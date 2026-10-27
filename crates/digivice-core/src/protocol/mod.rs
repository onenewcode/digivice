//! Radio & Bluetooth Protocol Models, HID Reporting, and Presence Tracking.

pub mod hid;
pub mod hid_session;
pub mod presence;

use heapless::Vec;

pub use hid::{
    CONSUMER_REPORT_ID, CONSUMER_REPORT_MAP, CONSUMER_REPORT_REFERENCE, ConsumerControlReport,
};
pub use hid_session::{
    HID_NOTIFY_TIMEOUT, HID_RELEASE_MAX_ATTEMPTS, HID_RELEASE_RETRY_DELAY, HidBlocked,
    HidControlCommand, HidPowerState, HidQueueOutcome, HidRecovery, HidSession, HidSubscription,
    queue_report_with_timeout,
};
pub use presence::{BlePresenceTracker, TargetBinding, TargetBindingError};

/// Operational health status of the 2.4GHz BLE subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadioStatus {
    /// Controller and GATT / scanning services are active.
    #[default]
    Active,
    /// Radio is disabled or running in local offline mode.
    Disabled,
    /// BleConnector::new failed during hardware startup.
    ControllerUnavailable,
    /// GATT server registration / initialization failed in peripheral task.
    GattFailed,
    /// Trouble Host HCI runner exited due to transport or controller error.
    RunnerExited,
}

impl RadioStatus {
    #[inline]
    pub const fn is_operational(self) -> bool {
        matches!(self, Self::Active)
    }

    #[inline]
    pub const fn is_fault(self) -> bool {
        matches!(
            self,
            Self::ControllerUnavailable | Self::GattFailed | Self::RunnerExited
        )
    }
}

/// Presence inferred from the bound BLE target, independent of the application UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresenceState {
    Near,
    #[default]
    Away,
}

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
pub enum BleBlipRole {
    Target,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BleBlip {
    pub hash: u16,
    pub rssi: i8,
    pub role: BleBlipRole,
}

#[derive(Debug, Clone)]
pub struct RadioReport {
    pub status: RadioStatus,
    pub target_bound: TargetBindingState,
    pub target_seen: TargetVisibility,
    pub presence: PresenceState,
    pub ble_rssi: i8,
    pub ble_blips: Vec<BleBlip, 12>,
}

impl RadioReport {
    #[inline]
    pub fn is_operational(&self) -> bool {
        self.status.is_operational()
    }

    #[inline]
    pub fn is_near(&self) -> bool {
        self.status.is_operational() && matches!(self.presence, PresenceState::Near)
    }

    #[inline]
    pub fn is_target_bound(&self) -> bool {
        self.target_bound.is_bound()
    }

    #[inline]
    pub fn is_target_seen(&self) -> bool {
        self.status.is_operational() && self.target_seen.is_visible()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BleAdvertisement {
    pub addr: [u8; 6],
    pub rssi: i8,
}
