//! 2.4GHz Wi-Fi 6 & BLE 5 Coexistence Radio Subsystem

pub mod ble_presence;
pub mod wifi_radar;

use crate::ui::{BleBlip, WifiApBlip};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use heapless::Vec;

pub use ble_presence::BlePresenceTracker;
pub use wifi_radar::WifiRadarAnalyzer;

#[derive(Debug, Clone)]
pub struct RadioReport {
    pub wifi_congestion: [u8; 13],
    pub wifi_blips: Vec<WifiApBlip, 8>,
    pub ble_near: bool,
    pub ble_rssi: i8,
    pub ble_blips: Vec<BleBlip, 12>,
}

pub type RadioReportChannel = Channel<CriticalSectionRawMutex, RadioReport, 4>;

pub static RADIO_REPORT_CHANNEL: RadioReportChannel = Channel::new();
