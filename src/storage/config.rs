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
            default_mode: ViewMode::Pet,
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
        let default_mode = ViewMode::try_from(mode_raw).unwrap_or(ViewMode::Pet);

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
    pub fn deserialize_legacy(buf: &[u8]) -> Result<Self, StorageError> {
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
            default_mode: ViewMode::try_from(mode_raw).unwrap_or(ViewMode::Pet),
            _reserved: reserved,
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
