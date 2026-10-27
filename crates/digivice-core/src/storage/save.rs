//! Bounded save coalescing. A request is not durable until its matching completion succeeds.

use embassy_time::{Duration, Instant};

use super::{ConfigLoadReport, DeviceConfig, StorageError};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

pub type SaveRequestSignal = Signal<CriticalSectionRawMutex, SaveRequest>;
pub type SaveCompletionSignal = Signal<CriticalSectionRawMutex, SaveCompletion>;

pub const SAVE_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveRequestId(u32);

impl SaveRequestId {
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveRequest {
    pub id: SaveRequestId,
    pub config: DeviceConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveCompletion {
    pub id: SaveRequestId,
    pub result: Result<DeviceConfig, StorageError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckDisposition {
    Accepted,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    Clean { seq: u32 },
    Dirty { seq: u32 },
    Saving { seq: u32 },
    Retrying { seq: u32, error: StorageError },
}

impl SaveStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Clean { .. } => "SYNC",
            Self::Dirty { .. } => "PENDING",
            Self::Saving { .. } => "WRITING",
            Self::Retrying { .. } => "RETRY",
        }
    }

    pub const fn committed_seq(self) -> u32 {
        match self {
            Self::Clean { seq }
            | Self::Dirty { seq }
            | Self::Saving { seq }
            | Self::Retrying { seq, .. } => seq,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitCertainty {
    Known,
    Uncertain,
}

pub struct SaveCoordinator {
    desired: DeviceConfig,
    committed: DeviceConfig,
    in_flight: Option<SaveRequest>,
    next_id: u32,
    next_allowed: Option<Instant>,
    last_error: Option<StorageError>,
    certainty: CommitCertainty,
}

impl SaveCoordinator {
    pub fn from_load(loaded: ConfigLoadReport, now: Instant) -> Self {
        let mut saves = Self::new(loaded.config, now);
        if loaded.requires_recovery() {
            saves.last_error = Some(StorageError::RecoveryRequired);
            saves.certainty = CommitCertainty::Uncertain;
        }
        saves
    }

    pub fn new(initial: DeviceConfig, now: Instant) -> Self {
        Self {
            desired: initial,
            committed: initial,
            in_flight: None,
            next_id: 0,
            next_allowed: now.checked_add(SAVE_INTERVAL),
            last_error: None,
            certainty: CommitCertainty::Known,
        }
    }

    pub fn observe(&mut self, config: DeviceConfig) {
        self.desired = config;
    }

    pub fn committed_config(&self) -> DeviceConfig {
        self.committed
    }

    pub fn status(&self) -> SaveStatus {
        let seq = self.committed.seq;
        if self.in_flight.is_some() {
            SaveStatus::Saving { seq }
        } else if !self.needs_save() {
            SaveStatus::Clean { seq }
        } else if let Some(error) = self.last_error {
            SaveStatus::Retrying { seq, error }
        } else {
            SaveStatus::Dirty { seq }
        }
    }

    /// At most one unacknowledged request; newer changes remain coalesced in `desired`.
    pub fn begin_save(&mut self, now: Instant) -> Option<SaveRequest> {
        if self.in_flight.is_some() || !self.needs_save() || now < self.next_allowed? {
            return None;
        }
        let request = SaveRequest {
            id: SaveRequestId(self.next_id),
            config: self.desired,
        };
        self.next_id = self.next_id.wrapping_add(1);
        self.next_allowed = now.checked_add(SAVE_INTERVAL);
        self.in_flight = Some(request);
        Some(request)
    }

    pub fn acknowledge(&mut self, completion: SaveCompletion) -> AckDisposition {
        let Some(pending) = self.in_flight else {
            return AckDisposition::Stale;
        };
        if pending.id != completion.id {
            return AckDisposition::Stale;
        }
        self.in_flight = None;
        match completion.result {
            Ok(committed) if same_payload(committed, pending.config) => {
                self.committed = committed;
                self.last_error = None;
                self.certainty = CommitCertainty::Known;
            }
            result => {
                self.last_error = Some(result.err().unwrap_or(StorageError::WriteFailed));
                // A final read failure can hide a completed commit. Even if RAM reverted to the
                // old value, issue a verified repair rather than falsely declaring it clean.
                self.certainty = CommitCertainty::Uncertain;
            }
        }
        AckDisposition::Accepted
    }

    fn needs_save(&self) -> bool {
        self.certainty == CommitCertainty::Uncertain || !same_payload(self.desired, self.committed)
    }
}

fn same_payload(mut left: DeviceConfig, mut right: DeviceConfig) -> bool {
    left.seq = 0;
    right.seq = 0;
    left == right
}
