//! 16MB Flash Storage & Wireless Situation Blackbox
//!
//! Provides zero-unsafe, endian-deterministic serialization and storage abstractions
//! for system configuration and flight recorder snapshots.

use crate::ui::ViewMode;
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;

pub const CONFIG_FLASH_PRIMARY_OFFSET: u32 = 0x0040_0000; // Sector A (4MB mark)
pub const CONFIG_FLASH_SECONDARY_OFFSET: u32 = 0x0040_1000; // Sector B (4MB + 4KB mark)
pub const BLACKBOX_FLASH_OFFSET: u32 = 0x0041_0000;
pub const FLASH_SECTOR_SIZE: usize = 4096;
pub const BLACKBOX_SNAPSHOT_CAPACITY: u32 = 1000;

pub const CONFIG_MAGIC: u32 = 0x4953_4C44; // "ISLD"
pub const CONFIG_VERSION: u16 = 2;

const CONFIG_CRC_OFFSET: usize = 8;
const CONFIG_PAYLOAD_OFFSET: usize = 12;
const CONFIG_PAYLOAD_SIZE: usize = 36;
const CONFIG_COMMIT_OFFSET: usize = CONFIG_PAYLOAD_OFFSET + CONFIG_PAYLOAD_SIZE;
const CONFIG_COMMIT_MARKER: u32 = 0x434F_4D4D; // "COMM"

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    EraseFailed,
    WriteFailed,
    ReadFailed,
    CrcMismatch,
    IncompleteWrite,
    InvalidBleTarget,
    InvalidMagic,
    BufferTooSmall,
}

impl core::fmt::Display for StorageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EraseFailed => write!(f, "Flash sector erase operation failed"),
            Self::WriteFailed => write!(f, "Flash write operation failed"),
            Self::ReadFailed => write!(f, "Flash read operation failed"),
            Self::CrcMismatch => write!(f, "Configuration checksum mismatch"),
            Self::IncompleteWrite => write!(f, "Configuration commit marker is missing"),
            Self::InvalidBleTarget => write!(f, "BLE target hash must fit in 16 non-zero bits"),
            Self::InvalidMagic => write!(f, "Invalid configuration magic header"),
            Self::BufferTooSmall => write!(f, "Provided buffer is too small for serialization"),
        }
    }
}

impl core::error::Error for StorageError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceConfig {
    magic: u32,
    version: u16,
    pub seq: u32,
    pub gyro_bias_mdps: [i32; 3],
    pub accel_bias_mg: [i16; 3],
    pub ble_target_hash: u32,
    pub rssi_away_threshold: i8,
    pub brightness_normal: u8,
    pub default_mode: ViewMode,
    _reserved: [u8; 9],
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            magic: CONFIG_MAGIC,
            version: CONFIG_VERSION,
            seq: 0,
            gyro_bias_mdps: [0, 0, 0],
            accel_bias_mg: [0, 0, 0],
            ble_target_hash: 0,
            rssi_away_threshold: -78,
            brightness_normal: 80,
            default_mode: ViewMode::Island,
            _reserved: [0u8; 9],
        }
    }
}

impl DeviceConfig {
    pub const SERIALIZED_SIZE: usize = CONFIG_COMMIT_OFFSET + 4;

    /// Return configuration header magic number.
    #[inline]
    pub const fn magic(&self) -> u32 {
        self.magic
    }

    /// Return the on-flash schema version.
    #[inline]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Serialize configuration to a deterministic, little-endian byte array without `unsafe`.
    pub fn serialize(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut buf = [0u8; Self::SERIALIZED_SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&(CONFIG_PAYLOAD_SIZE as u16).to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET..CONFIG_PAYLOAD_OFFSET + 4]
            .copy_from_slice(&self.seq.to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 4..CONFIG_PAYLOAD_OFFSET + 8]
            .copy_from_slice(&self.gyro_bias_mdps[0].to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 8..CONFIG_PAYLOAD_OFFSET + 12]
            .copy_from_slice(&self.gyro_bias_mdps[1].to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 12..CONFIG_PAYLOAD_OFFSET + 16]
            .copy_from_slice(&self.gyro_bias_mdps[2].to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 16..CONFIG_PAYLOAD_OFFSET + 18]
            .copy_from_slice(&self.accel_bias_mg[0].to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 18..CONFIG_PAYLOAD_OFFSET + 20]
            .copy_from_slice(&self.accel_bias_mg[1].to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 20..CONFIG_PAYLOAD_OFFSET + 22]
            .copy_from_slice(&self.accel_bias_mg[2].to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 22..CONFIG_PAYLOAD_OFFSET + 26]
            .copy_from_slice(&self.ble_target_hash.to_le_bytes());
        buf[CONFIG_PAYLOAD_OFFSET + 26] = self.rssi_away_threshold as u8;
        buf[CONFIG_PAYLOAD_OFFSET + 27] = self.brightness_normal;
        buf[CONFIG_PAYLOAD_OFFSET + 28] = u8::from(self.default_mode);
        buf[CONFIG_PAYLOAD_OFFSET + 29..CONFIG_COMMIT_OFFSET].copy_from_slice(&self._reserved);
        let crc = crc32(&buf[CONFIG_PAYLOAD_OFFSET..CONFIG_COMMIT_OFFSET]);
        buf[CONFIG_CRC_OFFSET..CONFIG_PAYLOAD_OFFSET].copy_from_slice(&crc.to_le_bytes());
        buf[CONFIG_COMMIT_OFFSET..Self::SERIALIZED_SIZE]
            .copy_from_slice(&CONFIG_COMMIT_MARKER.to_le_bytes());
        buf
    }

    /// Deserialize configuration safely using chunk splitting to avoid hardcoded offset fragility.
    pub fn deserialize(buf: &[u8]) -> Result<Self, StorageError> {
        if buf.len() < Self::SERIALIZED_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let (magic_raw, rest) = buf
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let magic = u32::from_le_bytes(*magic_raw);
        if magic != CONFIG_MAGIC {
            return Err(StorageError::InvalidMagic);
        }

        let (version_raw, rest) = rest
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let version = u16::from_le_bytes(*version_raw);
        if version != CONFIG_VERSION {
            return Err(StorageError::InvalidMagic);
        }

        let (payload_len_raw, rest) = rest
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        if usize::from(u16::from_le_bytes(*payload_len_raw)) != CONFIG_PAYLOAD_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let (crc_raw, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let expected_crc = u32::from_le_bytes(*crc_raw);
        let (payload, rest) = rest
            .split_first_chunk::<CONFIG_PAYLOAD_SIZE>()
            .ok_or(StorageError::BufferTooSmall)?;
        if crc32(payload) != expected_crc {
            return Err(StorageError::CrcMismatch);
        }
        let (commit_raw, _) = rest
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        if u32::from_le_bytes(*commit_raw) != CONFIG_COMMIT_MARKER {
            return Err(StorageError::IncompleteWrite);
        }

        let (seq_raw, payload) = payload
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let seq = u32::from_le_bytes(*seq_raw);

        let (gx, payload) = payload
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (gy, payload) = payload
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (gz, payload) = payload
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let gyro_bias_mdps = [
            i32::from_le_bytes(*gx),
            i32::from_le_bytes(*gy),
            i32::from_le_bytes(*gz),
        ];

        let (ax, payload) = payload
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (ay, payload) = payload
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (az, payload) = payload
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let accel_bias_mg = [
            i16::from_le_bytes(*ax),
            i16::from_le_bytes(*ay),
            i16::from_le_bytes(*az),
        ];

        let (ble_hash, payload) = payload
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let ble_target_hash = u32::from_le_bytes(*ble_hash);
        if ble_target_hash > u16::MAX as u32 {
            return Err(StorageError::InvalidBleTarget);
        }

        let (&[rssi_raw, brightness_raw, mode_raw], payload) = payload
            .split_first_chunk::<3>()
            .ok_or(StorageError::BufferTooSmall)?;
        let rssi_away_threshold = (rssi_raw as i8).clamp(-120, 0);
        let brightness_normal = brightness_raw.clamp(0, 100);
        let default_mode = ViewMode::try_from(mode_raw).unwrap_or(ViewMode::Island);

        let (reserved_chunk, _) = payload
            .split_first_chunk::<9>()
            .ok_or(StorageError::BufferTooSmall)?;
        let mut reserved = [0u8; 9];
        reserved.copy_from_slice(reserved_chunk);

        Ok(Self {
            magic,
            version,
            seq,
            gyro_bias_mdps,
            accel_bias_mg,
            ble_target_hash,
            rssi_away_threshold,
            brightness_normal,
            default_mode,
            _reserved: reserved,
        })
    }

    /// Parse a single-sector legacy (v1) record.
    fn deserialize_legacy(buf: &[u8]) -> Result<Self, StorageError> {
        const LEGACY_SIZE: usize = 44;
        if buf.len() < LEGACY_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let (magic_raw, rest) = buf
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        if u32::from_le_bytes(*magic_raw) != CONFIG_MAGIC {
            return Err(StorageError::InvalidMagic);
        }

        let (gx, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (gy, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (gz, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (ax, rest) = rest
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (ay, rest) = rest
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (az, rest) = rest
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (_, rest) = rest
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (ble_hash, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(StorageError::BufferTooSmall)?;
        let ble_target_hash = u32::from_le_bytes(*ble_hash);
        if ble_target_hash > u16::MAX as u32 {
            return Err(StorageError::InvalidBleTarget);
        }
        let (&[rssi_raw, brightness_raw, mode_raw], rest) = rest
            .split_first_chunk::<3>()
            .ok_or(StorageError::BufferTooSmall)?;
        let (reserved_chunk, _) = rest
            .split_first_chunk::<13>()
            .ok_or(StorageError::BufferTooSmall)?;
        let mut reserved = [0u8; 9];
        reserved.copy_from_slice(&reserved_chunk[..9]);

        Ok(Self {
            magic: CONFIG_MAGIC,
            version: CONFIG_VERSION,
            seq: 0,
            gyro_bias_mdps: [
                i32::from_le_bytes(*gx),
                i32::from_le_bytes(*gy),
                i32::from_le_bytes(*gz),
            ],
            accel_bias_mg: [
                i16::from_le_bytes(*ax),
                i16::from_le_bytes(*ay),
                i16::from_le_bytes(*az),
            ],
            ble_target_hash,
            rssi_away_threshold: (rssi_raw as i8).clamp(-120, 0),
            brightness_normal: brightness_raw.clamp(0, 100),
            default_mode: ViewMode::try_from(mode_raw).unwrap_or(ViewMode::Island),
            _reserved: reserved,
        })
    }
}

fn deserialize_config_record(buf: &[u8]) -> Result<DeviceConfig, StorageError> {
    match DeviceConfig::deserialize(buf) {
        Ok(config) => Ok(config),
        Err(v2_error) => {
            // Never interpret a damaged v2 record as legacy data. The legacy format has no
            // version field, so the version/length header is the only safe discriminator.
            let looks_like_v2 = buf.len() >= 8
                && u16::from_le_bytes([buf[4], buf[5]]) == CONFIG_VERSION
                && usize::from(u16::from_le_bytes([buf[6], buf[7]])) == CONFIG_PAYLOAD_SIZE;
            if looks_like_v2 {
                Err(v2_error)
            } else {
                DeviceConfig::deserialize_legacy(buf)
            }
        }
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::*;

    fn sample_config() -> DeviceConfig {
        DeviceConfig {
            magic: CONFIG_MAGIC,
            version: CONFIG_VERSION,
            seq: 7,
            gyro_bias_mdps: [1, -2, 3],
            accel_bias_mg: [4, -5, 6],
            ble_target_hash: 0xABCD,
            rssi_away_threshold: -80,
            brightness_normal: 65,
            default_mode: ViewMode::Pet,
            _reserved: [0u8; 9],
        }
    }

    #[test]
    fn config_round_trip_preserves_fields() {
        let config = sample_config();
        let encoded = config.serialize();
        let decoded = DeviceConfig::deserialize(&encoded).expect("valid config should decode");
        assert_eq!(decoded, config);
    }

    #[test]
    fn config_rejects_corrupted_payload() {
        let config = sample_config();
        let mut encoded = config.serialize();
        encoded[CONFIG_PAYLOAD_OFFSET] ^= 0xFF;
        assert_eq!(
            DeviceConfig::deserialize(&encoded),
            Err(StorageError::CrcMismatch)
        );
    }

    #[test]
    fn config_requires_commit_marker() {
        let config = sample_config();
        let mut encoded = config.serialize();
        encoded[CONFIG_COMMIT_OFFSET..].fill(0xFF);
        assert_eq!(
            DeviceConfig::deserialize(&encoded),
            Err(StorageError::IncompleteWrite)
        );
    }

    #[test]
    fn config_rejects_out_of_range_target_hash() {
        let mut config = sample_config();
        config.ble_target_hash = u32::from(u16::MAX) + 1;
        let encoded = config.serialize();
        assert!(DeviceConfig::deserialize(&encoded).is_err());
    }

    #[test]
    fn incomplete_v2_record_is_not_migrated_as_legacy() {
        let mut encoded = sample_config().serialize();
        encoded[CONFIG_COMMIT_OFFSET..].fill(0xFF);
        assert_eq!(
            deserialize_config_record(&encoded),
            Err(StorageError::IncompleteWrite)
        );
    }
}

#[inline]
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn config_payload_is_valid(buf: &[u8], expected_seq: u32) -> bool {
    if buf.len() != CONFIG_COMMIT_OFFSET {
        return false;
    }
    let mut full = [0u8; DeviceConfig::SERIALIZED_SIZE];
    full[..CONFIG_COMMIT_OFFSET].copy_from_slice(buf);
    full[CONFIG_COMMIT_OFFSET..].copy_from_slice(&CONFIG_COMMIT_MARKER.to_le_bytes());
    DeviceConfig::deserialize(&full)
        .map(|config| config.seq == expected_seq)
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TriggerReason {
    #[default]
    ManualDoubleTap = 0,
    HostError = 1,
    PowerDip = 2,
    HostDump = 3,
    Unknown(u8),
}

impl From<u8> for TriggerReason {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::ManualDoubleTap,
            1 => Self::HostError,
            2 => Self::PowerDip,
            3 => Self::HostDump,
            other => Self::Unknown(other),
        }
    }
}

impl From<TriggerReason> for u8 {
    fn from(reason: TriggerReason) -> Self {
        match reason {
            TriggerReason::ManualDoubleTap => 0,
            TriggerReason::HostError => 1,
            TriggerReason::PowerDip => 2,
            TriggerReason::HostDump => 3,
            TriggerReason::Unknown(code) => code,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlackboxSnapshot {
    pub timestamp_sec: u64,
    pub v_rail_mv: u16,
    pub wifi_congestion: [u8; 13],
    pub trigger_reason: TriggerReason,
    pub ap_count: u8,
    pub ble_count: u8,
    pub _reserved: [u8; 38], // Pad to exact 64 bytes
}

impl BlackboxSnapshot {
    pub const SERIALIZED_SIZE: usize = 64;

    /// Serialize snapshot into a fixed 64-byte payload.
    pub fn serialize(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut buf = [0u8; Self::SERIALIZED_SIZE];
        buf[0..8].copy_from_slice(&self.timestamp_sec.to_le_bytes());
        buf[8..10].copy_from_slice(&self.v_rail_mv.to_le_bytes());
        buf[10..23].copy_from_slice(&self.wifi_congestion);
        buf[23] = u8::from(self.trigger_reason);
        buf[24] = self.ap_count;
        buf[25] = self.ble_count;
        buf[26..64].copy_from_slice(&self._reserved);
        buf
    }

    /// Deserialize 64-byte record from Flash using slice chunk patterns.
    pub fn deserialize(buf: &[u8]) -> Result<Self, StorageError> {
        if buf.len() < Self::SERIALIZED_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let (ts, rest) = buf
            .split_first_chunk::<8>()
            .ok_or(StorageError::BufferTooSmall)?;
        let timestamp_sec = u64::from_le_bytes(*ts);

        let (v_rail, rest) = rest
            .split_first_chunk::<2>()
            .ok_or(StorageError::BufferTooSmall)?;
        let v_rail_mv = u16::from_le_bytes(*v_rail);

        let (wifi_chunk, rest) = rest
            .split_first_chunk::<13>()
            .ok_or(StorageError::BufferTooSmall)?;
        let mut wifi_congestion = [0u8; 13];
        wifi_congestion.copy_from_slice(wifi_chunk);

        let (&[trigger_raw, ap_count, ble_count], rest) = rest
            .split_first_chunk::<3>()
            .ok_or(StorageError::BufferTooSmall)?;
        let trigger_reason = TriggerReason::from(trigger_raw);

        let (reserved_chunk, _rest) = rest
            .split_first_chunk::<38>()
            .ok_or(StorageError::BufferTooSmall)?;
        let mut reserved = [0u8; 38];
        reserved.copy_from_slice(reserved_chunk);

        Ok(Self {
            timestamp_sec,
            v_rail_mv,
            wifi_congestion,
            trigger_reason,
            ap_count,
            ble_count,
            _reserved: reserved,
        })
    }
}

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
            if self.flash.read(offset, &mut buf).is_err() || buf.iter().all(|byte| *byte == 0xFF) {
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
        if verify != bytes {
            return Err(StorageError::WriteFailed);
        }

        self.next_snapshot_index = (slot + 1) % BLACKBOX_SNAPSHOT_CAPACITY;
        Ok(slot)
    }
}
