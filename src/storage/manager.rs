//! Atomic Ping-Pong Flash Storage Manager for Configuration and Telemetry Snapshots.

use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;

use crate::storage::blackbox::BlackboxSnapshot;
use crate::storage::config::{
    CONFIG_COMMIT_MARKER, CONFIG_COMMIT_OFFSET, DeviceConfig, config_payload_is_valid,
    deserialize_config_record,
};
use crate::storage::{
    BLACKBOX_FLASH_OFFSET, BLACKBOX_SNAPSHOT_CAPACITY, CONFIG_FLASH_PRIMARY_OFFSET,
    CONFIG_FLASH_SECONDARY_OFFSET, FLASH_SECTOR_SIZE, StorageError,
};

pub struct StorageManager<'d> {
    flash: FlashStorage<'d>,
    active_sector_offset: u32,
    latest_seq: u32,
    next_snapshot_index: u32,
}

impl<'d> StorageManager<'d> {
    pub fn new(flash_peri: FLASH<'d>) -> Self {
        Self {
            flash: FlashStorage::new(flash_peri),
            active_sector_offset: CONFIG_FLASH_PRIMARY_OFFSET,
            latest_seq: 0,
            next_snapshot_index: 0,
        }
    }

    /// Load device configuration using ping-pong dual sector recovery (Sector A and Sector B).
    /// Recovers gracefully even if power was lost during an erase or write operation.
    pub fn load_config(&mut self) -> DeviceConfig {
        let res_a = self.read_config_sector(CONFIG_FLASH_PRIMARY_OFFSET);
        let res_b = self.read_config_sector(CONFIG_FLASH_SECONDARY_OFFSET);

        let (active_sector_offset, latest_seq, config) = match (res_a, res_b) {
            (Some(cfg_a), Some(cfg_b)) => {
                // Both sectors valid: select newer sequence counter with wrapping support
                if cfg_b.seq.wrapping_sub(cfg_a.seq) < 0x8000_0000 && cfg_b.seq != cfg_a.seq {
                    (CONFIG_FLASH_SECONDARY_OFFSET, cfg_b.seq, cfg_b)
                } else {
                    (CONFIG_FLASH_PRIMARY_OFFSET, cfg_a.seq, cfg_a)
                }
            }
            (Some(cfg_a), None) => (CONFIG_FLASH_PRIMARY_OFFSET, cfg_a.seq, cfg_a),
            (None, Some(cfg_b)) => (CONFIG_FLASH_SECONDARY_OFFSET, cfg_b.seq, cfg_b),
            (None, None) => (CONFIG_FLASH_PRIMARY_OFFSET, 0, DeviceConfig::default()),
        };

        self.active_sector_offset = active_sector_offset;
        self.latest_seq = latest_seq;
        self.next_snapshot_index = self.find_next_snapshot_slot();
        config
    }

    fn read_config_sector(&mut self, offset: u32) -> Option<DeviceConfig> {
        let mut buf = [0u8; DeviceConfig::SERIALIZED_SIZE];
        if self.flash.read(offset, &mut buf).is_err() {
            return None;
        }

        deserialize_config_record(&buf).ok()
    }

    /// Save device configuration using atomic double-buffering.
    /// Writes to the inactive sector first; if power is cut, the active sector remains 100% intact.
    pub fn save_config(&mut self, config: &DeviceConfig) -> Result<DeviceConfig, StorageError> {
        if config.ble_target_hash > u16::MAX as u32 {
            return Err(StorageError::InvalidBleTarget);
        }

        let target_offset = if self.active_sector_offset == CONFIG_FLASH_PRIMARY_OFFSET {
            CONFIG_FLASH_SECONDARY_OFFSET
        } else {
            CONFIG_FLASH_PRIMARY_OFFSET
        };

        let next_seq = self.latest_seq.wrapping_add(1);
        let mut to_save = *config;
        to_save.seq = next_seq;

        // Erase the inactive target sector
        self.flash
            .erase(target_offset, target_offset + (FLASH_SECTOR_SIZE as u32))
            .map_err(|_| StorageError::EraseFailed)?;

        // Write the new record without its commit marker. This prevents a torn write from being
        // mistaken for a valid configuration during the next boot.
        let bytes = to_save.serialize();
        self.flash
            .write(target_offset, &bytes[..CONFIG_COMMIT_OFFSET])
            .map_err(|_| StorageError::WriteFailed)?;

        // Verify the payload before publishing it with the commit marker.
        let mut verify_buf = [0u8; DeviceConfig::SERIALIZED_SIZE];
        self.flash
            .read(target_offset, &mut verify_buf[..CONFIG_COMMIT_OFFSET])
            .map_err(|_| StorageError::ReadFailed)?;
        if !config_payload_is_valid(&verify_buf[..CONFIG_COMMIT_OFFSET], next_seq) {
            return Err(StorageError::WriteFailed);
        }

        self.flash
            .write(
                target_offset + CONFIG_COMMIT_OFFSET as u32,
                &CONFIG_COMMIT_MARKER.to_le_bytes(),
            )
            .map_err(|_| StorageError::WriteFailed)?;

        self.flash
            .read(target_offset, &mut verify_buf)
            .map_err(|_| StorageError::ReadFailed)?;
        let verified =
            DeviceConfig::deserialize(&verify_buf).map_err(|_| StorageError::WriteFailed)?;
        if verified.seq != next_seq {
            return Err(StorageError::WriteFailed);
        }

        self.active_sector_offset = target_offset;
        self.latest_seq = next_seq;
        Ok(verified)
    }

    fn find_next_snapshot_slot(&mut self) -> u32 {
        let mut buf = [0u8; BlackboxSnapshot::SERIALIZED_SIZE];
        for slot in 0..BLACKBOX_SNAPSHOT_CAPACITY {
            let offset = BLACKBOX_FLASH_OFFSET + slot * (BlackboxSnapshot::SERIALIZED_SIZE as u32);
            if self.flash.read(offset, &mut buf).is_err() {
                return slot;
            }
            if buf.iter().all(|byte| *byte == 0xFF) {
                return slot;
            }
            if BlackboxSnapshot::deserialize(&buf).is_err() {
                let sector_start = offset & !(FLASH_SECTOR_SIZE as u32 - 1);
                if self
                    .flash
                    .erase(sector_start, sector_start + FLASH_SECTOR_SIZE as u32)
                    .is_ok()
                {
                    return (sector_start - BLACKBOX_FLASH_OFFSET)
                        / BlackboxSnapshot::SERIALIZED_SIZE as u32;
                }
                return slot;
            }
        }
        0
    }

    /// Append a 64-byte blackbox snapshot and advance the circular cursor.
    /// A sector is erased only when the cursor wraps onto its first slot.
    pub fn record_snapshot(&mut self, snapshot: &BlackboxSnapshot) -> Result<u32, StorageError> {
        let slot = self.next_snapshot_index % BLACKBOX_SNAPSHOT_CAPACITY;
        let offset = BLACKBOX_FLASH_OFFSET + (slot * (BlackboxSnapshot::SERIALIZED_SIZE as u32));

        let sector_start = offset & !(FLASH_SECTOR_SIZE as u32 - 1);
        if offset.is_multiple_of(FLASH_SECTOR_SIZE as u32) {
            self.flash
                .erase(sector_start, sector_start + (FLASH_SECTOR_SIZE as u32))
                .map_err(|_| StorageError::EraseFailed)?;
        }

        let bytes = snapshot.serialize();
        self.flash
            .write_nor(offset, &bytes)
            .map_err(|_| StorageError::WriteFailed)?;

        let mut verify = [0u8; BlackboxSnapshot::SERIALIZED_SIZE];
        self.flash
            .read(offset, &mut verify)
            .map_err(|_| StorageError::ReadFailed)?;
        if verify != bytes || BlackboxSnapshot::deserialize(&verify).is_err() {
            let sector_start = offset & !(FLASH_SECTOR_SIZE as u32 - 1);
            let _ = self
                .flash
                .erase(sector_start, sector_start + FLASH_SECTOR_SIZE as u32);
            self.next_snapshot_index =
                (sector_start - BLACKBOX_FLASH_OFFSET) / BlackboxSnapshot::SERIALIZED_SIZE as u32;
            return Err(StorageError::WriteFailed);
        }

        self.next_snapshot_index = (slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY;
        Ok(slot)
    }
}
