//! 2.4GHz BLE 5 Radio Subsystem

pub mod ble_presence;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::{channel::Channel, signal::Signal};
use heapless::Vec;

pub use ble_presence::{BlePresenceTracker, TargetBinding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleTargetCommand {
    Bind(TargetBinding),
    Clear,
}

impl From<BleTargetCommand> for Option<TargetBinding> {
    fn from(command: BleTargetCommand) -> Self {
        match command {
            BleTargetCommand::Bind(binding) => Some(binding),
            BleTargetCommand::Clear => None,
        }
    }
}

pub use crate::proto::PresenceState;

/// Strongly-typed Bluetooth target binding state to eliminate boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetBindingState {
    #[default]
    Unbound,
    Bound,
}

impl TargetBindingState {
    #[inline]
    pub const fn is_bound(&self) -> bool {
        matches!(self, Self::Bound)
    }
}

impl From<bool> for TargetBindingState {
    #[inline]
    fn from(bound: bool) -> Self {
        if bound { Self::Bound } else { Self::Unbound }
    }
}

/// Strongly-typed Bluetooth target scan visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetVisibility {
    #[default]
    Hidden,
    Visible,
}

impl TargetVisibility {
    #[inline]
    pub const fn is_visible(&self) -> bool {
        matches!(self, Self::Visible)
    }
}

impl From<bool> for TargetVisibility {
    #[inline]
    fn from(visible: bool) -> Self {
        if visible { Self::Visible } else { Self::Hidden }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BleBlip {
    pub hash: u16,
    pub rssi: i8,
    pub is_target: bool,
}

#[derive(Debug, Clone)]
pub struct RadioReport {
    pub target_bound: TargetBindingState,
    pub target_seen: TargetVisibility,
    pub presence: PresenceState,
    pub ble_rssi: i8,
    pub ble_blips: Vec<BleBlip, 12>,
}

impl RadioReport {
    #[inline]
    pub fn is_near(&self) -> bool {
        matches!(self.presence, PresenceState::Near)
    }

    #[inline]
    pub fn is_target_bound(&self) -> bool {
        self.target_bound.is_bound()
    }

    #[inline]
    pub fn is_target_seen(&self) -> bool {
        self.target_seen.is_visible()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BleAdvertisement {
    pub addr: [u8; 6],
    pub rssi: i8,
}

pub type RadioReportChannel = Channel<CriticalSectionRawMutex, RadioReport, 4>;
pub type BleAdvertisementChannel = Channel<CriticalSectionRawMutex, BleAdvertisement, 32>;

pub static RADIO_REPORT_CHANNEL: RadioReportChannel = Channel::new();
pub static BLE_ADVERTISEMENT_CHANNEL: BleAdvertisementChannel = Channel::new();
pub static RADIO_TARGET_SIGNAL: Signal<CriticalSectionRawMutex, BleTargetCommand> = Signal::new();
