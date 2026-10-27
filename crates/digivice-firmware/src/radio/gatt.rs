//! BLE GATT Server services and advertisement event handler.

#![allow(
    clippy::large_stack_frames,
    reason = "Generated GATT constructors own fixed BLE service storage"
)]

use trouble_host::prelude::*;

use crate::ipc::BLE_ADVERTISEMENT_CHANNEL;
use crate::radio::BleAdvertisement;
use crate::radio::hid::{CONSUMER_REPORT_MAP, CONSUMER_REPORT_REFERENCE, ConsumerControlReport};

#[gatt_server(connections_max = 1, attribute_table_size = 64)]
pub struct DigiviceGattServer {
    pub hid: HidService,
    pub music: MusicService,
}

#[gatt_service(uuid = "1812")]
pub struct HidService {
    // Normally connectable, but no RemoteWake support while suspended.
    #[characteristic(uuid = "2a4a", read, value = [0x11, 0x01, 0x00, 0x02])]
    pub information: [u8; 4],
    #[characteristic(uuid = "2a4b", read, value = CONSUMER_REPORT_MAP)]
    pub report_map: [u8; 25],
    #[characteristic(uuid = "2a4c", write_without_response, value = 0)]
    pub control_point: u8,
    // The ID belongs to Report Reference; the GATT value is only the 16-bit usage.
    #[characteristic(uuid = "2a4d", read, notify, value = ConsumerControlReport::release_bytes())]
    #[descriptor(uuid = "2908", read, value = CONSUMER_REPORT_REFERENCE)]
    pub report: [u8; 2],
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

use core::sync::atomic::{AtomicU32, Ordering};

pub static ADV_DROPPED_PACKETS: AtomicU32 = AtomicU32::new(0);

pub struct AdvertisementHandler;

impl EventHandler for AdvertisementHandler {
    fn on_adv_reports(&self, reports: LeAdvReportsIter<'_>) {
        for report in reports.flatten() {
            let advertisement = BleAdvertisement {
                addr: report.addr.into_inner(),
                rssi: report.rssi,
            };
            if BLE_ADVERTISEMENT_CHANNEL.try_send(advertisement).is_err() {
                ADV_DROPPED_PACKETS.fetch_add(1, Ordering::Relaxed);
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
                ADV_DROPPED_PACKETS.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
