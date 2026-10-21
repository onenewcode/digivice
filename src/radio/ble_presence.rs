//! BLE 5 Presence Detection & Beacon Tracker

use super::{BleBlip, TargetVisibility};
use crate::proto::PresenceState;
use embassy_time::{Duration, Instant};
use heapless::Vec;

pub struct BlePresenceTracker {
    pub target_hash: Option<u16>,
    pub smoothed_target_rssi: i16,
    pub presence: PresenceState,
    pub target_visibility: TargetVisibility,
    pub last_seen_target: Instant,
    pub blips: Vec<BleBlip, 12>,
    pub rssi_near_threshold: i8,
    pub rssi_away_threshold: i8,
}

/// Validated 16-bit BLE target identifier persisted in device configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetBinding(u16);

impl TargetBinding {
    pub const fn new(hash: u16) -> Option<Self> {
        if hash == 0 { None } else { Some(Self(hash)) }
    }

    pub const fn hash(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetBindingError {
    OutOfRange,
    Zero,
}

impl core::fmt::Display for TargetBindingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfRange => f.write_str("BLE target hash exceeds 16 bits"),
            Self::Zero => f.write_str("BLE target hash must be non-zero"),
        }
    }
}

impl core::error::Error for TargetBindingError {}

impl TryFrom<u32> for TargetBinding {
    type Error = TargetBindingError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let hash = u16::try_from(value).map_err(|_| TargetBindingError::OutOfRange)?;
        TargetBinding::new(hash).ok_or(TargetBindingError::Zero)
    }
}

impl core::fmt::Display for TargetBinding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl BlePresenceTracker {
    pub fn new(target: Option<TargetBinding>) -> Self {
        Self {
            target_hash: target.map(TargetBinding::hash),
            smoothed_target_rssi: -85,
            presence: PresenceState::Away,
            target_visibility: TargetVisibility::Hidden,
            last_seen_target: Instant::now(),
            blips: Vec::new(),
            rssi_near_threshold: -68,
            rssi_away_threshold: -78,
        }
    }

    #[inline]
    pub fn is_near(&self) -> bool {
        matches!(self.presence, PresenceState::Near)
    }

    pub fn set_target(&mut self, binding: Option<TargetBinding>) {
        self.target_hash = binding.map(TargetBinding::hash);
        self.smoothed_target_rssi = -85;
        self.presence = PresenceState::Away;
        self.target_visibility = TargetVisibility::Hidden;
        self.blips.clear();
        self.last_seen_target = Instant::now();
    }

    /// Process a detected BLE advertisement report
    pub fn process_advertisement(&mut self, addr_bytes: &[u8; 6], rssi: i8, now: Instant) {
        // Hash the 6-byte address to 16-bit
        let hash = ((addr_bytes[0] as u16) ^ (addr_bytes[2] as u16) ^ (addr_bytes[4] as u16))
            | (((addr_bytes[1] as u16) ^ (addr_bytes[3] as u16) ^ (addr_bytes[5] as u16)) << 8);

        let is_target = match self.target_hash {
            Some(t) => t == hash,
            None => false,
        };

        // Only a bound target contributes to presence state; binding is explicit over host protocol.
        if is_target {
            self.last_seen_target = now;
            self.target_visibility = TargetVisibility::Visible;
            // Zero-FPU Integer Exponential Moving Average: new = (old * 3 + rssi) / 4
            self.smoothed_target_rssi =
                (((self.smoothed_target_rssi as i32 * 3) + (rssi as i32)) / 4) as i16;
            if self.smoothed_target_rssi >= (self.rssi_near_threshold as i16) {
                self.presence = PresenceState::Near;
            } else if self.smoothed_target_rssi < (self.rssi_away_threshold as i16) {
                self.presence = PresenceState::Away;
            }
        }

        // Update BLE Target Blip Table
        if let Some(existing) = self.blips.iter_mut().find(|b| b.hash == hash) {
            existing.rssi = rssi;
            existing.is_target = is_target;
        } else {
            if self.blips.is_full() {
                self.blips.remove(0);
            }
            let _ = self.blips.push(BleBlip {
                hash,
                rssi,
                is_target,
            });
        }
    }

    /// Check watchdog (if target not seen for > 15s -> declare AWAY)
    pub fn update_watchdog(&mut self, now: Instant) -> PresenceState {
        if self.target_hash.is_some()
            && self.target_visibility.is_visible()
            && now.duration_since(self.last_seen_target) > Duration::from_secs(15)
        {
            self.presence = PresenceState::Away;
        }
        self.presence
    }

    /// Bind the currently strongest device as target
    pub fn bind_strongest(&mut self) -> Option<u16> {
        if let Some(strongest) = self.blips.iter().max_by_key(|b| b.rssi) {
            self.target_hash = Some(strongest.hash);
            self.smoothed_target_rssi = strongest.rssi as i16;
            self.presence = PresenceState::Near;
            self.target_visibility = TargetVisibility::Visible;
            self.last_seen_target = Instant::now();
            Some(strongest.hash)
        } else {
            None
        }
    }
}
