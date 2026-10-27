//! Application configuration state kept alongside the persisted device settings.

use crate::radio::TargetBinding;
use crate::storage::DeviceConfig;

/// Device configuration plus the BLE binding that is currently active in RAM.
///
/// `ble_target_hash` follows the radio's accepted state instead of a queued command.
/// This is RAM state; the separate save status reports whether it has been committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigState {
    config: DeviceConfig,
    active_target: Option<TargetBinding>,
}

impl ConfigState {
    pub fn from_persisted(config: DeviceConfig) -> Self {
        let active_target = if config.ble_target_hash == 0 {
            None
        } else {
            config.ble_target_hash.try_into().ok()
        };
        Self {
            config,
            active_target,
        }
    }

    pub const fn config(&self) -> DeviceConfig {
        self.config
    }

    pub const fn active_target(&self) -> Option<TargetBinding> {
        self.active_target
    }

    pub fn set_active_target(&mut self, target: Option<TargetBinding>) {
        self.config.ble_target_hash = target.map(TargetBinding::hash).unwrap_or(0) as u32;
        self.active_target = target;
    }
}
