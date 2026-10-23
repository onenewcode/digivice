//! BLE GATT Server services and advertisement event handler.

#![allow(
    clippy::large_stack_frames,
    reason = "Generated GATT constructors own fixed BLE service storage"
)]

use esp_println::println;
use trouble_host::prelude::*;

use crate::ipc::BLE_ADVERTISEMENT_CHANNEL;
use crate::radio::BleAdvertisement;

#[gatt_server(connections_max = 2, attribute_table_size = 64)]
pub struct DigiviceGattServer {
    pub hid: HidService,
    pub music: MusicService,
}

#[gatt_service(uuid = "1812")]
pub struct HidService {
    #[characteristic(uuid = "2a4a", read, value = [0x11, 0x01, 0x00, 0x03])]
    pub information: [u8; 4],
    #[characteristic(
        uuid = "2a4b",
        read,
        value = [
            0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x01, 0x15, 0x00, 0x26, 0xFF, 0x03,
            0x75, 0x10, 0x95, 0x01, 0x19, 0x00, 0x2A, 0xFF, 0x03, 0x81, 0x00, 0xC0,
        ]
    )]
    pub report_map: [u8; 25],
    #[characteristic(uuid = "2a4c", write, value = 0)]
    pub control_point: u8,
    // The report map declares report ID 1, so every input report includes the ID byte.
    #[characteristic(uuid = "2a4d", read, notify, value = [1, 0, 0])]
    #[descriptor(uuid = "2908", read, value = [1, 1])]
    pub report: [u8; 3],
    #[characteristic(uuid = "2a4e", read, write, value = 1)]
    pub protocol_mode: u8,
}

#[gatt_service(uuid = "f0e10000-9b7a-4a9d-8e76-6d6c8e4a0001")]
pub struct MusicService {
    #[characteristic(
        uuid = "f0e10001-9b7a-4a9d-8e76-6d6c8e4a0001",
        write,
        value = [0; 10]
    )]
    pub scene: [u8; 10],
}

pub struct AdvertisementHandler;

impl EventHandler for AdvertisementHandler {
    fn on_adv_reports(&self, reports: LeAdvReportsIter<'_>) {
        for report in reports.flatten() {
            let advertisement = BleAdvertisement {
                addr: report.addr.into_inner(),
                rssi: report.rssi,
            };
            if BLE_ADVERTISEMENT_CHANNEL.try_send(advertisement).is_err() {
                println!("WARN: BLE_ADVERTISEMENT_CHANNEL full");
            }
        }
    }

    fn on_ext_adv_reports(&self, reports: LeExtAdvReportsIter<'_>) {
        for report in reports.flatten() {
            let advertisement = BleAdvertisement {
                addr: report.addr.into_inner(),
                rssi: report.rssi,
            };
            if BLE_ADVERTISEMENT_CHANNEL.try_send(advertisement).is_err() {
                println!("WARN: BLE_ADVERTISEMENT_CHANNEL full");
            }
        }
    }
}
