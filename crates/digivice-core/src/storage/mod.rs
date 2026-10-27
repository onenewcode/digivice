//! 16MB Flash Storage & Wireless Situation Blackbox
//!
//! Provides zero-unsafe, endian-deterministic serialization and storage abstractions
//! for system configuration and flight recorder snapshots.

pub mod blackbox;
pub mod config;
pub mod crc;
pub mod manager;
pub mod save;

pub use blackbox::{BlackboxSnapshot, TriggerReason};
pub use config::{
    CONFIG_COMMIT_MARKER, CONFIG_COMMIT_OFFSET, CONFIG_CRC_OFFSET, CONFIG_MAGIC,
    CONFIG_PAYLOAD_OFFSET, CONFIG_PAYLOAD_SIZE, CONFIG_VERSION, DeviceConfig,
};
pub use crc::crc32;
pub use manager::{
    ConfigLoadReport, ConfigSchema, ConfigSectorStatus, ConfigSource, StorageManager,
};
pub use save::{
    AckDisposition, SAVE_INTERVAL, SaveCompletion, SaveCompletionSignal, SaveCoordinator,
    SaveRequest, SaveRequestId, SaveRequestSignal, SaveStatus,
};

pub const CONFIG_FLASH_PRIMARY_OFFSET: u32 = 0x0040_0000; // Sector A (4MB mark)
pub const CONFIG_FLASH_SECONDARY_OFFSET: u32 = 0x0040_1000; // Sector B (4MB + 4KB mark)
pub const BLACKBOX_FLASH_OFFSET: u32 = 0x0041_0000;
pub const FLASH_SECTOR_SIZE: usize = 4096;
pub const BLACKBOX_SNAPSHOT_CAPACITY: u32 = 1000;

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
    RecoveryRequired,
    UnsupportedVersion,
    InvalidPetState,
    InvalidConfig,
}

impl core::fmt::Display for StorageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EraseFailed => write!(f, "Flash sector erase operation failed"),
            Self::WriteFailed => write!(f, "Flash write operation failed"),
            Self::ReadFailed => write!(f, "Flash read operation failed"),
            Self::CrcMismatch => write!(f, "Storage record checksum mismatch"),
            Self::IncompleteWrite => write!(f, "Configuration commit marker is missing"),
            Self::InvalidBleTarget => write!(f, "BLE target hash must fit in 16 non-zero bits"),
            Self::InvalidMagic => write!(f, "Invalid configuration magic header"),
            Self::BufferTooSmall => write!(f, "Provided buffer is too small for serialization"),
            Self::RecoveryRequired => {
                write!(f, "Configuration reads incomplete; refusing to erase")
            }
            Self::UnsupportedVersion => write!(f, "Unsupported configuration version"),
            Self::InvalidPetState => write!(f, "Invalid persistent pet state"),
            Self::InvalidConfig => write!(f, "Configuration violates supported value ranges"),
        }
    }
}

impl core::error::Error for StorageError {}
