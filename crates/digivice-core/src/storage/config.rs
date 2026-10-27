//! Flash configuration Schema V3, with read-only V1/V2 migration.

use crate::domain::{
    CalendarDay, HealthStatus, NurtureProgress, PetArchive, PetArchiveData, ViewMode,
};
use crate::storage::StorageError;
use crate::storage::crc::crc32;

pub const CONFIG_MAGIC: u32 = 0x4953_4C44; // "ISLD"
pub const CONFIG_VERSION: u16 = 3;
pub const CONFIG_V2_VERSION: u16 = 2;
pub const CONFIG_V2_PAYLOAD_SIZE: usize = 36;

pub const CONFIG_CRC_OFFSET: usize = 8;
pub const CONFIG_PAYLOAD_OFFSET: usize = 12;
pub const CONFIG_PAYLOAD_SIZE: usize = 68;
pub const CONFIG_COMMIT_OFFSET: usize = CONFIG_PAYLOAD_OFFSET + CONFIG_PAYLOAD_SIZE;
pub const CONFIG_COMMIT_MARKER: u32 = 0x434F_4D4D; // "COMM"
const LEGACY_CONFIG_SIZE: usize = 44;

#[derive(Clone, Copy, PartialEq, Eq)]
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
    pub pet_care_mistakes: u8,
    pub pet_training_count: u16,
    pub pet_evolution_stage: u8,
    pub pet_weight_g: u8,
    pub pet_age_days: u16,
    pub pet_health: HealthStatus,
    pub pet_poop_count: u8,
    pub pet_energy: u8,
    pub pet_affinity: u16,
    pub pet_nurture_progress: NurtureProgress,
    pub pet_calendar_day: Option<CalendarDay>,
}

impl core::fmt::Debug for DeviceConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceConfig")
            .field("version", &self.version)
            .field("seq", &self.seq)
            .field("stage", &self.pet_evolution_stage)
            .field("training", &self.pet_training_count)
            .field("age_days", &self.pet_age_days)
            .field("health", &self.pet_health)
            .finish_non_exhaustive()
    }
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
            pet_care_mistakes: 0,
            pet_training_count: 0,
            pet_evolution_stage: 0,
            pet_weight_g: 5,
            pet_age_days: 1,
            pet_health: HealthStatus::Healthy,
            pet_poop_count: 0,
            pet_energy: 100,
            pet_affinity: 0,
            pet_nurture_progress: NurtureProgress::default(),
            pet_calendar_day: None,
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

    pub fn pet_archive(&self) -> PetArchive {
        PetArchive::new_clamped(PetArchiveData {
            stage: self.pet_evolution_stage,
            hunger: self.pet_hunger,
            strength: self.pet_strength,
            weight_g: self.pet_weight_g,
            care_mistakes: self.pet_care_mistakes,
            training_count: self.pet_training_count,
            age_days: self.pet_age_days,
            health: self.pet_health,
            poop_count: self.pet_poop_count,
            energy: self.pet_energy,
            affinity: self.pet_affinity,
            progress: self.pet_nurture_progress,
            calendar_day: self.pet_calendar_day,
        })
    }

    pub fn set_pet_archive(&mut self, archive: PetArchive) {
        let data = archive.data();
        self.pet_evolution_stage = data.stage;
        self.pet_hunger = data.hunger;
        self.pet_strength = data.strength;
        self.pet_weight_g = data.weight_g;
        self.pet_care_mistakes = data.care_mistakes;
        self.pet_training_count = data.training_count;
        self.pet_age_days = data.age_days;
        self.pet_health = data.health;
        self.pet_poop_count = data.poop_count;
        self.pet_energy = data.energy;
        self.pet_affinity = data.affinity;
        self.pet_nurture_progress = data.progress;
        self.pet_calendar_day = data.calendar_day;
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.ble_target_hash > u16::MAX as u32 {
            return Err(StorageError::InvalidBleTarget);
        }
        if !(-120..=0).contains(&self.rssi_away_threshold)
            || self.brightness_normal > 100
            || self.pet_hunger > 4
            || self.pet_strength > 4
            || self.pet_evolution_stage > 5
            || self.pet_weight_g == 0
            || self.pet_age_days == 0
            || self.pet_poop_count > 4
            || self.pet_energy > 100
        {
            return Err(StorageError::InvalidConfig);
        }
        Ok(())
    }

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
        // V2 discipline and the old 8-bit training byte are no longer authoritative.
        buf[CONFIG_PAYLOAD_OFFSET + 32] = self.pet_care_mistakes;
        buf[CONFIG_PAYLOAD_OFFSET + 34] = self.pet_evolution_stage;
        buf[CONFIG_PAYLOAD_OFFSET + 35] = self.pet_weight_g;
        put_u16_le(&mut buf, 48, self.pet_training_count);
        put_u16_le(&mut buf, 50, self.pet_age_days);
        buf[52] = self.pet_health as u8;
        buf[53] = self.pet_poop_count;
        buf[54] = self.pet_energy;
        put_u16_le(&mut buf, 56, self.pet_affinity);
        put_u32_le(&mut buf, 60, self.pet_nurture_progress.digestion_ms());
        put_u32_le(&mut buf, 64, self.pet_nurture_progress.starvation_ms());
        put_u32_le(&mut buf, 68, self.pet_nurture_progress.uncleaned_ms());
        put_u32_le(&mut buf, 72, self.pet_nurture_progress.unattended_ms());
        put_u32_le(
            &mut buf,
            76,
            self.pet_calendar_day
                .map_or(u32::MAX, CalendarDay::day_count),
        );
        let crc = crc32(&buf[CONFIG_PAYLOAD_OFFSET..CONFIG_COMMIT_OFFSET]);
        put_u32_le(&mut buf, CONFIG_CRC_OFFSET, crc);
        put_u32_le(&mut buf, CONFIG_COMMIT_OFFSET, CONFIG_COMMIT_MARKER);
        buf
    }

    /// Decode a current record; callers loading Flash should use the version dispatcher.
    pub fn deserialize(buf: &[u8]) -> Result<Self, StorageError> {
        Self::deserialize_versioned(buf, VersionedSchema::V3)
    }

    fn deserialize_versioned(buf: &[u8], schema: VersionedSchema) -> Result<Self, StorageError> {
        let commit_offset = CONFIG_PAYLOAD_OFFSET + schema.payload_size();
        if buf.len() < commit_offset + 4 {
            return Err(StorageError::BufferTooSmall);
        }
        if get_u32_le(buf, 0) != CONFIG_MAGIC {
            return Err(StorageError::InvalidMagic);
        }
        let version = get_u16_le(buf, 4);
        if version != schema.version() {
            return Err(StorageError::UnsupportedVersion);
        }
        if usize::from(get_u16_le(buf, 6)) != schema.payload_size() {
            return Err(StorageError::BufferTooSmall);
        }
        if crc32(&buf[CONFIG_PAYLOAD_OFFSET..commit_offset]) != get_u32_le(buf, CONFIG_CRC_OFFSET) {
            return Err(StorageError::CrcMismatch);
        }
        if get_u32_le(buf, commit_offset) != CONFIG_COMMIT_MARKER {
            return Err(StorageError::IncompleteWrite);
        }

        let mut config = Self::read_common_fields(buf)?;
        match schema {
            VersionedSchema::V2 => config.pet_training_count = u16::from(buf[45]),
            VersionedSchema::V3 => {
                config.pet_training_count = get_u16_le(buf, 48);
                config.pet_age_days = get_u16_le(buf, 50).max(1);
                config.pet_health =
                    HealthStatus::try_from(buf[52]).map_err(|_| StorageError::InvalidPetState)?;
                config.pet_poop_count = buf[53].min(4);
                config.pet_energy = buf[54].min(100);
                config.pet_affinity = get_u16_le(buf, 56);
                config.pet_nurture_progress = NurtureProgress::new_clamped(
                    get_u32_le(buf, 60),
                    get_u32_le(buf, 64),
                    get_u32_le(buf, 68),
                    get_u32_le(buf, 72),
                );
                let day = get_u32_le(buf, 76);
                config.pet_calendar_day = if day == u32::MAX {
                    None
                } else {
                    Some(CalendarDay::from_day_count(day).ok_or(StorageError::InvalidPetState)?)
                };
            }
        }
        Ok(config)
    }

    fn read_common_fields(buf: &[u8]) -> Result<Self, StorageError> {
        let ble_target_hash = get_u32_le(buf, 34);
        if ble_target_hash > u16::MAX as u32 {
            return Err(StorageError::InvalidBleTarget);
        }
        Ok(Self {
            seq: get_u32_le(buf, 12),
            gyro_bias_mdps: [
                get_i32_le(buf, 16),
                get_i32_le(buf, 20),
                get_i32_le(buf, 24),
            ],
            accel_bias_mg: [
                get_i16_le(buf, 28),
                get_i16_le(buf, 30),
                get_i16_le(buf, 32),
            ],
            ble_target_hash,
            rssi_away_threshold: (buf[38] as i8).clamp(-120, 0),
            brightness_normal: buf[39].min(100),
            default_mode: ViewMode::try_from(buf[40]).unwrap_or(ViewMode::Pet),
            pet_hunger: buf[41].min(4),
            pet_strength: buf[42].min(4),
            pet_care_mistakes: buf[44],
            pet_evolution_stage: buf[46].min(5),
            pet_weight_g: if buf[47] == 0 { 5 } else { buf[47] },
            ..Self::default()
        })
    }

    /// Parse a single-sector legacy (v1) record.
    pub fn deserialize_legacy(buf: &[u8]) -> Result<Self, StorageError> {
        if buf.len() < LEGACY_CONFIG_SIZE {
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
            ..Self::default()
        })
    }
}

#[derive(Clone, Copy)]
enum VersionedSchema {
    V2,
    V3,
}

impl VersionedSchema {
    const fn version(self) -> u16 {
        match self {
            Self::V2 => CONFIG_V2_VERSION,
            Self::V3 => CONFIG_VERSION,
        }
    }

    const fn payload_size(self) -> usize {
        match self {
            Self::V2 => CONFIG_V2_PAYLOAD_SIZE,
            Self::V3 => CONFIG_PAYLOAD_SIZE,
        }
    }

    fn from_header(buf: &[u8]) -> Option<Self> {
        if buf.len() < 8 {
            return None;
        }
        match (get_u16_le(buf, 4), usize::from(get_u16_le(buf, 6))) {
            (CONFIG_V2_VERSION, CONFIG_V2_PAYLOAD_SIZE) => Some(Self::V2),
            (CONFIG_VERSION, CONFIG_PAYLOAD_SIZE) => Some(Self::V3),
            _ => None,
        }
    }
}

pub fn deserialize_config_record(buf: &[u8]) -> Result<DeviceConfig, StorageError> {
    if let Some(schema) = VersionedSchema::from_header(buf) {
        return DeviceConfig::deserialize_versioned(buf, schema);
    }
    // An unversioned 44-byte record cannot have programmed current-format metadata after it.
    let legacy_tail_is_erased = buf
        .get(LEGACY_CONFIG_SIZE..)
        .is_some_and(|tail| tail.iter().all(|byte| *byte == 0xff));
    if legacy_tail_is_erased {
        DeviceConfig::deserialize_legacy(buf)
    } else {
        DeviceConfig::deserialize(buf)
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
