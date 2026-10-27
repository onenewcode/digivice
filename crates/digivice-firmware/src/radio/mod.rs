//! 2.4GHz BLE 5 Radio Subsystem
//!
//! Provides Bluetooth Low Energy advertising, scanning, GATT server, HID consumer
//! control reporting, and target presence tracking.

pub mod gatt;

pub use digivice_core::protocol::hid;
pub use digivice_core::protocol::hid_session;
pub use digivice_core::protocol::presence;
pub use digivice_core::protocol::*;

pub use gatt::{AdvertisementHandler, DigiviceGattServer, HidService, MusicService};

// Re-export centralized IPC channels for radio
pub use crate::ipc::{
    BLE_ADVERTISEMENT_CHANNEL, BleAdvertisementChannel, RADIO_REPORT_CHANNEL, RADIO_STATUS_SIGNAL,
    RADIO_TARGET_SIGNAL, RadioReportChannel, TARGET_APPLIED_SIGNAL,
};
