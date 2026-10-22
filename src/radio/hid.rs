//! BLE HID Consumer Control report structures and payload formatting.

use crate::domain::HidAction;

/// BLE HID Consumer Control report with report ID 1 and a 16-bit usage payload.
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

    pub const fn bytes(self) -> [u8; 3] {
        let [lo, hi] = self.usage.to_le_bytes();
        [1, lo, hi]
    }

    /// Release the currently pressed consumer-control usage.
    pub const fn release_bytes() -> [u8; 3] {
        [1, 0, 0]
    }
}
