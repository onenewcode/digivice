//! Atomic Ping-Pong Flash Storage Manager for Configuration and Telemetry Snapshots.

use embedded_storage::nor_flash::NorFlash;

use crate::storage::blackbox::BlackboxSnapshot;
use crate::storage::config::{
    CONFIG_COMMIT_MARKER, CONFIG_COMMIT_OFFSET, CONFIG_PAYLOAD_SIZE, CONFIG_V2_PAYLOAD_SIZE,
    CONFIG_V2_VERSION, CONFIG_VERSION, DeviceConfig, config_payload_is_valid,
    deserialize_config_record,
};
use crate::storage::{
    BLACKBOX_FLASH_OFFSET, BLACKBOX_SNAPSHOT_CAPACITY, CONFIG_FLASH_PRIMARY_OFFSET,
    CONFIG_FLASH_SECONDARY_OFFSET, FLASH_SECTOR_SIZE, SaveCompletion, SaveRequest, StorageError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Defaults,
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSchema {
    LegacyV1,
    Version2,
    Current,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSectorStatus {
    Empty,
    Valid { schema: ConfigSchema, seq: u32 },
    Invalid(StorageError),
    ReadFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigLoadReport {
    pub config: DeviceConfig,
    pub source: ConfigSource,
    pub primary: ConfigSectorStatus,
    pub secondary: ConfigSectorStatus,
}

impl ConfigLoadReport {
    pub const fn requires_recovery(&self) -> bool {
        matches!(self.primary, ConfigSectorStatus::ReadFailed)
            || matches!(self.secondary, ConfigSectorStatus::ReadFailed)
    }
}

struct SectorRead {
    config: Option<DeviceConfig>,
    status: ConfigSectorStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfigRecovery {
    Required,
    Ready,
}

const UNINITIALIZED_SLOT: u16 = u16::MAX;

pub struct StorageManager<F> {
    flash: F,
    active_sector_offset: u32,
    latest_seq: u32,
    next_snapshot_slot: u16,
    next_snapshot_seq: u32,
    recovery: ConfigRecovery,
}

impl<F: NorFlash> StorageManager<F> {
    pub fn new(flash: F) -> Self {
        Self {
            flash,
            active_sector_offset: CONFIG_FLASH_PRIMARY_OFFSET,
            latest_seq: 0,
            next_snapshot_slot: UNINITIALIZED_SLOT,
            next_snapshot_seq: 1,
            recovery: ConfigRecovery::Required,
        }
    }

    /// Load device configuration using ping-pong dual sector recovery (Sector A and Sector B).
    /// Recovers gracefully even if power was lost during an erase or write operation.
    pub fn load_config(&mut self) -> DeviceConfig {
        self.load_config_report().config
    }

    /// Read-only configuration recovery; blackbox scanning is deferred until explicitly used.
    pub fn load_config_report(&mut self) -> ConfigLoadReport {
        let res_a = self.read_config_sector(CONFIG_FLASH_PRIMARY_OFFSET);
        let res_b = self.read_config_sector(CONFIG_FLASH_SECONDARY_OFFSET);

        let mut config = res_a.config.unwrap_or_default();
        let mut source = if res_a.config.is_some() {
            ConfigSource::Primary
        } else {
            ConfigSource::Defaults
        };
        if let Some(secondary) = res_b.config {
            let newer =
                secondary.seq != config.seq && secondary.seq.wrapping_sub(config.seq) < 0x8000_0000;
            if source == ConfigSource::Defaults || newer {
                config = secondary;
                source = ConfigSource::Secondary;
            }
        }

        self.active_sector_offset = if source == ConfigSource::Secondary {
            CONFIG_FLASH_SECONDARY_OFFSET
        } else {
            CONFIG_FLASH_PRIMARY_OFFSET
        };
        self.latest_seq = config.seq;
        let report = ConfigLoadReport {
            config,
            source,
            primary: res_a.status,
            secondary: res_b.status,
        };
        self.recovery = if report.requires_recovery() {
            ConfigRecovery::Required
        } else {
            ConfigRecovery::Ready
        };
        report
    }

    fn read_config_sector(&mut self, offset: u32) -> SectorRead {
        let mut buf = [0u8; DeviceConfig::SERIALIZED_SIZE];
        if self.flash.read(offset, &mut buf).is_err() {
            return SectorRead {
                config: None,
                status: ConfigSectorStatus::ReadFailed,
            };
        }
        if buf.iter().all(|byte| *byte == 0xff) {
            return SectorRead {
                config: None,
                status: ConfigSectorStatus::Empty,
            };
        }
        match deserialize_config_record(&buf) {
            Ok(config) => {
                let schema = if u16::from_le_bytes([buf[4], buf[5]]) == CONFIG_VERSION
                    && usize::from(u16::from_le_bytes([buf[6], buf[7]])) == CONFIG_PAYLOAD_SIZE
                {
                    ConfigSchema::Current
                } else if u16::from_le_bytes([buf[4], buf[5]]) == CONFIG_V2_VERSION
                    && usize::from(u16::from_le_bytes([buf[6], buf[7]])) == CONFIG_V2_PAYLOAD_SIZE
                {
                    ConfigSchema::Version2
                } else {
                    ConfigSchema::LegacyV1
                };
                SectorRead {
                    config: Some(config),
                    status: ConfigSectorStatus::Valid {
                        schema,
                        seq: config.seq,
                    },
                }
            }
            Err(error) => SectorRead {
                config: None,
                status: ConfigSectorStatus::Invalid(error),
            },
        }
    }

    /// Save device configuration using atomic double-buffering.
    /// Writes to the inactive sector first; if power is cut, the active sector remains 100% intact.
    pub fn save_config(&mut self, config: &DeviceConfig) -> Result<DeviceConfig, StorageError> {
        if self.recovery != ConfigRecovery::Ready {
            return Err(StorageError::RecoveryRequired);
        }
        config.validate()?;
        let target_offset = if self.active_sector_offset == CONFIG_FLASH_PRIMARY_OFFSET {
            CONFIG_FLASH_SECONDARY_OFFSET
        } else {
            CONFIG_FLASH_PRIMARY_OFFSET
        };
        let mut to_save = *config;
        to_save.seq = self.latest_seq.wrapping_add(1);
        self.program_config_payload(target_offset, &to_save)?;
        let verified = self.commit_config_record(target_offset, &to_save)?;
        self.active_sector_offset = target_offset;
        self.latest_seq = verified.seq;
        Ok(verified)
    }

    fn program_config_payload(
        &mut self,
        offset: u32,
        config: &DeviceConfig,
    ) -> Result<(), StorageError> {
        self.flash
            .erase(offset, offset + FLASH_SECTOR_SIZE as u32)
            .map_err(|_| StorageError::EraseFailed)?;
        // NorFlash::write programs erased words without an implicit sector read/erase/rewrite.
        let bytes = config.serialize();
        self.flash
            .write(offset, &bytes[..CONFIG_COMMIT_OFFSET])
            .map_err(|_| StorageError::WriteFailed)?;
        let mut verify = [0; CONFIG_COMMIT_OFFSET];
        self.flash
            .read(offset, &mut verify)
            .map_err(|_| StorageError::ReadFailed)?;
        if !config_payload_is_valid(&verify, config.seq) {
            return Err(StorageError::WriteFailed);
        }
        Ok(())
    }

    fn commit_config_record(
        &mut self,
        offset: u32,
        expected: &DeviceConfig,
    ) -> Result<DeviceConfig, StorageError> {
        self.flash
            .write(
                offset + CONFIG_COMMIT_OFFSET as u32,
                &CONFIG_COMMIT_MARKER.to_le_bytes(),
            )
            .map_err(|_| StorageError::WriteFailed)?;
        let mut verify = [0; DeviceConfig::SERIALIZED_SIZE];
        self.flash
            .read(offset, &mut verify)
            .map_err(|_| StorageError::ReadFailed)?;
        let verified = DeviceConfig::deserialize(&verify).map_err(|_| StorageError::WriteFailed)?;
        if verified != *expected {
            return Err(StorageError::WriteFailed);
        }
        Ok(verified)
    }

    pub fn execute_save(&mut self, request: SaveRequest) -> SaveCompletion {
        SaveCompletion {
            id: request.id,
            result: self.save_config(&request.config),
        }
    }

    fn scan_blackbox_cursor(&mut self) -> (u32, u32) {
        let mut newest: Option<(u32, u32)> = None;
        let mut buf = [0u8; BlackboxSnapshot::SERIALIZED_SIZE];

        for slot in 0..BLACKBOX_SNAPSHOT_CAPACITY {
            let offset = BLACKBOX_FLASH_OFFSET + slot * (BlackboxSnapshot::SERIALIZED_SIZE as u32);
            if self.flash.read(offset, &mut buf).is_err() {
                continue;
            }
            if buf.iter().all(|byte| *byte == 0xFF) {
                continue;
            }
            if let Ok(snapshot) = BlackboxSnapshot::deserialize(&buf) {
                match newest {
                    None => newest = Some((slot, snapshot.seq)),
                    Some((_, max_seq)) => {
                        if crate::storage::blackbox::is_seq_newer(snapshot.seq, max_seq) {
                            newest = Some((slot, snapshot.seq));
                        }
                    }
                }
            }
            // Damaged/corrupt slots are safely skipped without erasing their parent sector
        }

        match newest {
            None => (0, 1),
            Some((last_slot, last_seq)) => {
                let next_slot = (last_slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY;
                let next_seq = last_seq.wrapping_add(1);
                (next_slot, next_seq)
            }
        }
    }

    /// Append a 64-byte blackbox snapshot and advance the circular cursor.
    /// A sector is erased only when the cursor wraps onto its first slot.
    pub fn record_snapshot(&mut self, snapshot: &BlackboxSnapshot) -> Result<u32, StorageError> {
        let (slot, seq) = if self.next_snapshot_slot != UNINITIALIZED_SLOT {
            (self.next_snapshot_slot as u32, self.next_snapshot_seq)
        } else {
            let (index, seq) = self.scan_blackbox_cursor();
            self.next_snapshot_slot = index as u16;
            self.next_snapshot_seq = seq;
            (index, seq)
        };

        let offset = BLACKBOX_FLASH_OFFSET + (slot * (BlackboxSnapshot::SERIALIZED_SIZE as u32));
        let sector_start = offset & !(FLASH_SECTOR_SIZE as u32 - 1);

        // Erase sector only when entering its first slot
        if offset.is_multiple_of(FLASH_SECTOR_SIZE as u32) {
            self.flash
                .erase(sector_start, sector_start + (FLASH_SECTOR_SIZE as u32))
                .map_err(|_| StorageError::EraseFailed)?;
        }

        // Before writing, verify that the slot is clean (all 0xFF).
        // If it is dirty (e.g. from an earlier interrupted write or skipped corrupted slot),
        // we advance past it rather than erasing the entire sector and destroying neighboring records!
        let mut slot_check = [0u8; BlackboxSnapshot::SERIALIZED_SIZE];
        if self.flash.read(offset, &mut slot_check).is_ok()
            && !slot_check.iter().all(|byte| *byte == 0xFF)
        {
            self.next_snapshot_slot = ((slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY) as u16;
            return Err(StorageError::WriteFailed);
        }

        let mut stamped = *snapshot;
        stamped.seq = seq;
        let bytes = stamped.serialize();

        if self.flash.write(offset, &bytes).is_err() {
            self.next_snapshot_slot = ((slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY) as u16;
            self.next_snapshot_seq = seq.wrapping_add(1);
            return Err(StorageError::WriteFailed);
        }

        let mut verify = [0u8; BlackboxSnapshot::SERIALIZED_SIZE];
        if self.flash.read(offset, &mut verify).is_err() {
            self.next_snapshot_slot = ((slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY) as u16;
            self.next_snapshot_seq = seq.wrapping_add(1);
            return Err(StorageError::ReadFailed);
        }

        if verify != bytes || BlackboxSnapshot::deserialize(&verify).is_err() {
            // Write verification failed: advance cursor without erasing the sector,
            // preserving all other valid snapshots in this sector!
            self.next_snapshot_slot = ((slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY) as u16;
            self.next_snapshot_seq = seq.wrapping_add(1);
            return Err(StorageError::WriteFailed);
        }

        self.next_snapshot_slot = ((slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY) as u16;
        self.next_snapshot_seq = seq.wrapping_add(1);
        Ok(slot)
    }

    /// Read back a stored blackbox snapshot by slot index.
    pub fn read_snapshot(&mut self, slot: u32) -> Result<BlackboxSnapshot, StorageError> {
        if slot >= BLACKBOX_SNAPSHOT_CAPACITY {
            return Err(StorageError::BufferTooSmall);
        }
        let offset = BLACKBOX_FLASH_OFFSET + (slot * (BlackboxSnapshot::SERIALIZED_SIZE as u32));
        let mut buf = [0u8; BlackboxSnapshot::SERIALIZED_SIZE];
        self.flash
            .read(offset, &mut buf)
            .map_err(|_| StorageError::ReadFailed)?;
        BlackboxSnapshot::deserialize(&buf)
    }
}
