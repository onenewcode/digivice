//! Flash Device Configuration Record (Schema V2 with V1 legacy migration).

use crate::domain::ViewMode;
use crate::storage::StorageError;
use crate::storage::crc::crc32;

pub const CONFIG_MAGIC: u32 = 0x4953_4C44; // "ISLD"
pub const CONFIG_VERSION: u16 = 2;

pub const CONFIG_CRC_OFFSET: usize = 8;
pub const CONFIG_PAYLOAD_OFFSET: usize = 12;
pub const CONFIG_PAYLOAD_SIZE: usize = 36;
pub const CONFIG_COMMIT_OFFSET: usize = CONFIG_PAYLOAD_OFFSET + CONFIG_PAYLOAD_SIZE;
pub const CONFIG_COMMIT_MARKER: u32 = 0x434F_4D4D; // "COMM"

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
    pub pet_hunger: u8,
    pub pet_strength: u8,
    pub pet_discipline: u8,
    pub pet_care_mistakes: u8,
    pub pet_training_count: u8,
    pub pet_evolution_stage: u8,
    pub _reserved: [u8; 1],
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
            default_mode: ViewMode::Pet,
            pet_hunger: 4,
            pet_strength: 4,
            pet_discipline: 50,
            pet_care_mistakes: 0,
            pet_training_count: 0,
            pet_evolution_stage: 0,
            _reserved: [0u8; 1],
        }
    }
}

#[inline(always)]
fn put_u16_le(buf: &mut [u8], offset: usize, val: u16) {
    let bytes = val.to_le_bytes();
    buf[offset] = bytes[0];
    buf[offset + 1] = bytes[1];
}

#[inline(always)]
fn put_u32_le(buf: &mut [u8], offset: usize, val: u32) {
    let bytes = val.to_le_bytes();
    buf[offset] = bytes[0];
    buf[offset + 1] = bytes[1];
    buf[offset + 2] = bytes[2];
    buf[offset + 3] = bytes[3];
}

#[inline(always)]
fn put_i16_le(buf: &mut [u8], offset: usize, val: i16) {
    put_u16_le(buf, offset, val as u16);
}

#[inline(always)]
fn put_i32_le(buf: &mut [u8], offset: usize, val: i32) {
    put_u32_le(buf, offset, val as u32);
}

#[inline(always)]
fn get_u16_le(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

#[inline(always)]
fn get_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

#[inline(always)]
fn get_i16_le(buf: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([buf[offset], buf[offset + 1]])
}

#[inline(always)]
fn get_i32_le(buf: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
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
        put_u32_le(&mut buf, 0, self.magic);
        put_u16_le(&mut buf, 4, self.version);
        put_u16_le(&mut buf, 6, CONFIG_PAYLOAD_SIZE as u16);
        put_u32_le(&mut buf, CONFIG_PAYLOAD_OFFSET, self.seq);
        put_i32_le(&mut buf, CONFIG_PAYLOAD_OFFSET + 4, self.gyro_bias_mdps[0]);
        put_i32_le(&mut buf, CONFIG_PAYLOAD_OFFSET + 8, self.gyro_bias_mdps[1]);
        put_i32_le(&mut buf, CONFIG_PAYLOAD_OFFSET + 12, self.gyro_bias_mdps[2]);
        put_i16_le(&mut buf, CONFIG_PAYLOAD_OFFSET + 16, self.accel_bias_mg[0]);
        put_i16_le(&mut buf, CONFIG_PAYLOAD_OFFSET + 18, self.accel_bias_mg[1]);
        put_i16_le(&mut buf, CONFIG_PAYLOAD_OFFSET + 20, self.accel_bias_mg[2]);
        put_u32_le(&mut buf, CONFIG_PAYLOAD_OFFSET + 22, self.ble_target_hash);
        buf[CONFIG_PAYLOAD_OFFSET + 26] = self.rssi_away_threshold as u8;
        buf[CONFIG_PAYLOAD_OFFSET + 27] = self.brightness_normal;
        buf[CONFIG_PAYLOAD_OFFSET + 28] = u8::from(self.default_mode);
        buf[CONFIG_PAYLOAD_OFFSET + 29] = self.pet_hunger;
        buf[CONFIG_PAYLOAD_OFFSET + 30] = self.pet_strength;
        buf[CONFIG_PAYLOAD_OFFSET + 31] = self.pet_discipline;
        buf[CONFIG_PAYLOAD_OFFSET + 32] = self.pet_care_mistakes;
        buf[CONFIG_PAYLOAD_OFFSET + 33] = self.pet_training_count;
        buf[CONFIG_PAYLOAD_OFFSET + 34] = self.pet_evolution_stage;
        buf[CONFIG_PAYLOAD_OFFSET + 35] = self._reserved[0];
        let crc = crc32(&buf[CONFIG_PAYLOAD_OFFSET..CONFIG_COMMIT_OFFSET]);
        put_u32_le(&mut buf, CONFIG_CRC_OFFSET, crc);
        put_u32_le(&mut buf, CONFIG_COMMIT_OFFSET, CONFIG_COMMIT_MARKER);
        buf
    }

    /// Deserialize configuration safely using direct offset indexing to minimize stack frame size.
    pub fn deserialize(buf: &[u8]) -> Result<Self, StorageError> {
        if buf.len() < Self::SERIALIZED_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let magic = get_u32_le(buf, 0);
        if magic != CONFIG_MAGIC {
            return Err(StorageError::InvalidMagic);
        }

        let version = get_u16_le(buf, 4);
        if version != CONFIG_VERSION {
            return Err(StorageError::InvalidMagic);
        }

        let payload_len = usize::from(get_u16_le(buf, 6));
        if payload_len != CONFIG_PAYLOAD_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let expected_crc = get_u32_le(buf, CONFIG_CRC_OFFSET);
        let payload = &buf[CONFIG_PAYLOAD_OFFSET..CONFIG_COMMIT_OFFSET];
        if crc32(payload) != expected_crc {
            return Err(StorageError::CrcMismatch);
        }

        let commit_marker = get_u32_le(buf, CONFIG_COMMIT_OFFSET);
        if commit_marker != CONFIG_COMMIT_MARKER {
            return Err(StorageError::IncompleteWrite);
        }

        let seq = get_u32_le(buf, CONFIG_PAYLOAD_OFFSET);
        let gyro_bias_mdps = [
            get_i32_le(buf, CONFIG_PAYLOAD_OFFSET + 4),
            get_i32_le(buf, CONFIG_PAYLOAD_OFFSET + 8),
            get_i32_le(buf, CONFIG_PAYLOAD_OFFSET + 12),
        ];
        let accel_bias_mg = [
            get_i16_le(buf, CONFIG_PAYLOAD_OFFSET + 16),
            get_i16_le(buf, CONFIG_PAYLOAD_OFFSET + 18),
            get_i16_le(buf, CONFIG_PAYLOAD_OFFSET + 20),
        ];
        let ble_target_hash = get_u32_le(buf, CONFIG_PAYLOAD_OFFSET + 22);
        if ble_target_hash > u16::MAX as u32 {
            return Err(StorageError::InvalidBleTarget);
        }

        let rssi_raw = buf[CONFIG_PAYLOAD_OFFSET + 26];
        let brightness_raw = buf[CONFIG_PAYLOAD_OFFSET + 27];
        let mode_raw = buf[CONFIG_PAYLOAD_OFFSET + 28];

        let rssi_away_threshold = (rssi_raw as i8).clamp(-120, 0);
        let brightness_normal = brightness_raw.clamp(0, 100);
        let default_mode = ViewMode::try_from(mode_raw).unwrap_or(ViewMode::Pet);

        let pet_hunger = buf[CONFIG_PAYLOAD_OFFSET + 29].min(4);
        let pet_strength = buf[CONFIG_PAYLOAD_OFFSET + 30].min(4);
        let pet_discipline = buf[CONFIG_PAYLOAD_OFFSET + 31].min(100);
        let pet_care_mistakes = buf[CONFIG_PAYLOAD_OFFSET + 32];
        let pet_training_count = buf[CONFIG_PAYLOAD_OFFSET + 33];
        let pet_evolution_stage = buf[CONFIG_PAYLOAD_OFFSET + 34].min(5);
        let reserved = [buf[CONFIG_PAYLOAD_OFFSET + 35]];

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
            pet_hunger,
            pet_strength,
            pet_discipline,
            pet_care_mistakes,
            pet_training_count,
            pet_evolution_stage,
            _reserved: reserved,
        })
    }

    /// Parse a single-sector legacy (v1) record.
    pub fn deserialize_legacy(buf: &[u8]) -> Result<Self, StorageError> {
        const LEGACY_SIZE: usize = 44;
        if buf.len() < LEGACY_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let magic = get_u32_le(buf, 0);
        if magic != CONFIG_MAGIC {
            return Err(StorageError::InvalidMagic);
        }

        let gyro_bias_mdps = [get_i32_le(buf, 4), get_i32_le(buf, 8), get_i32_le(buf, 12)];
        let accel_bias_mg = [
            get_i16_le(buf, 16),
            get_i16_le(buf, 18),
            get_i16_le(buf, 20),
        ];
        let ble_target_hash = get_u32_le(buf, 24);
        if ble_target_hash > u16::MAX as u32 {
            return Err(StorageError::InvalidBleTarget);
        }

        let rssi_raw = buf[28];
        let brightness_raw = buf[29];
        let mode_raw = buf[30];

        Ok(Self {
            magic: CONFIG_MAGIC,
            version: CONFIG_VERSION,
            seq: 0,
            gyro_bias_mdps,
            accel_bias_mg,
            ble_target_hash,
            rssi_away_threshold: (rssi_raw as i8).clamp(-120, 0),
            brightness_normal: brightness_raw.clamp(0, 100),
            default_mode: ViewMode::try_from(mode_raw).unwrap_or(ViewMode::Pet),
            pet_hunger: 4,
            pet_strength: 4,
            pet_discipline: 50,
            pet_care_mistakes: 0,
            pet_training_count: 0,
            pet_evolution_stage: 0,
            _reserved: [0],
        })
    }
}

pub fn deserialize_config_record(buf: &[u8]) -> Result<DeviceConfig, StorageError> {
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

pub fn config_payload_is_valid(buf: &[u8], expected_seq: u32) -> bool {
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
