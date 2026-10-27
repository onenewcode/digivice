//! BLE HID Consumer Control report structures and payload formatting.

use crate::domain::HidAction;

pub const CONSUMER_REPORT_ID: u8 = 1;
/// HIDS Report Reference: report ID and Input Report type, not notification data.
pub const CONSUMER_REPORT_REFERENCE: [u8; 2] = [CONSUMER_REPORT_ID, 1];
pub const CONSUMER_REPORT_MAP: [u8; 25] = [
    0x05,
    0x0C,
    0x09,
    0x01,
    0xA1,
    0x01,
    0x85,
    CONSUMER_REPORT_ID,
    0x15,
    0x00,
    0x26,
    0xFF,
    0x03,
    0x75,
    0x10,
    0x95,
    0x01,
    0x19,
    0x00,
    0x2A,
    0xFF,
    0x03,
    0x81,
    0x00,
    0xC0,
];

/// BLE HID Consumer Control report carrying one 16-bit usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumerControlReport {
    pub usage: u16,
}

impl ConsumerControlReport {
    pub const fn from_action(action: HidAction) -> Self {
        let usage = match action {
            HidAction::PlayPause => 0x00CD,
            HidAction::PreviousTrack => 0x00B6,
            HidAction::NextTrack => 0x00B5,
            HidAction::VolumeUp => 0x00E9,
            HidAction::VolumeDown => 0x00EA,
            HidAction::Mute => 0x00E2,
        };
        Self { usage }
    }

    pub const fn bytes(self) -> [u8; 2] {
        // HOGP's host translation layer adds the ID; the GATT value excludes it.
        self.usage.to_le_bytes()
    }

    /// Release the currently pressed consumer-control usage.
    pub const fn release_bytes() -> [u8; 2] {
        [0, 0]
    }
}
