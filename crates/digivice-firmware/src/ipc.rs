//! Centralized inter-task communication channels and signals for Embassy concurrency.
//!
//! Provides statically allocated, bounded synchronization primitives connecting
//! the concurrent tasks of the Digivice system.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;

use crate::board::{ImuAttitudeSnapshot, PowerStatus};
use crate::domain::{ClockSnapshot, HidAction, MusicScene};
use crate::radio::{BleAdvertisement, BleTargetCommand, RadioReport, RadioStatus};
use crate::ui::UiState;

mod gesture;
pub use gesture::{GestureChannel, OrientationSignal, publish_gesture};
mod storage;
pub use storage::{SaveCompletionSignal, SaveRequestSignal};

// ============================================================================
// Bounded Inter-Task Event Channels
// ============================================================================

/// Radio presence reports emitted by `radio_presence_task` and drained by `app_state_task`.
pub type RadioReportChannel = Channel<CriticalSectionRawMutex, RadioReport, 4>;

/// Raw BLE advertisement stream emitted by BLE HCI scan handler and consumed by `radio_presence_task`.
pub type BleAdvertisementChannel = Channel<CriticalSectionRawMutex, BleAdvertisement, 32>;

/// Outgoing HID consumer control actions emitted by `app_state_task` and sent via `ble_peripheral_task`.
pub type HidActionChannel = Channel<CriticalSectionRawMutex, HidAction, 8>;

/// Incoming music scenes written via BLE GATT and forwarded to `app_state_task`.
pub type MusicSceneChannel = Channel<CriticalSectionRawMutex, MusicScene, 8>;

pub static GESTURE_CHANNEL: GestureChannel = Channel::new();
pub static RADIO_REPORT_CHANNEL: RadioReportChannel = Channel::new();
pub static BLE_ADVERTISEMENT_CHANNEL: BleAdvertisementChannel = Channel::new();
pub static HID_ACTION_CHANNEL: HidActionChannel = Channel::new();
pub static MUSIC_SCENE_CHANNEL: MusicSceneChannel = Channel::new();

// ============================================================================
// Single-Slot Overwriting Signals (Latest Value Semantics)
// ============================================================================

/// UI presentation snapshot published by `app_state_task` and rendered by `ui_render_task`.
pub static UI_STATE_SIGNAL: Signal<CriticalSectionRawMutex, UiState> = Signal::new();

/// Debounced orientation is safety state, never a lossy gesture queue entry.
pub static ORIENTATION_SIGNAL: OrientationSignal = OrientationSignal::new();

/// RTC calendar and health updated atomically by `rtc_power_task`.
pub static TIME_SIGNAL: Signal<CriticalSectionRawMutex, ClockSnapshot> = Signal::new();

/// Battery rail voltage and charge status updated by `sensor_task` and read by `app_state_task`.
pub static POWER_SIGNAL: Signal<CriticalSectionRawMutex, PowerStatus> = Signal::new();

/// High-rate IMU attitude snapshot updated by `sensor_task` and read by `app_state_task`.
pub static IMU_SIGNAL: Signal<CriticalSectionRawMutex, ImuAttitudeSnapshot> = Signal::new();

/// Command to bind or clear target BLE MAC hash, dispatched to `radio_presence_task`.
pub static RADIO_TARGET_SIGNAL: Signal<CriticalSectionRawMutex, BleTargetCommand> = Signal::new();

/// Status updates from BLE tasks (e.g. runner exit or GATT failure) dispatched to `radio_presence_task`.
pub static RADIO_STATUS_SIGNAL: Signal<CriticalSectionRawMutex, RadioStatus> = Signal::new();

/// Confirmation signal sent from `radio_presence_task` back to `app_state_task` when target applied.
pub static TARGET_APPLIED_SIGNAL: Signal<CriticalSectionRawMutex, BleTargetCommand> = Signal::new();

/// Only `SaveCoordinator` produces requests, with at most one awaiting completion.
pub static SAVE_REQUEST_SIGNAL: SaveRequestSignal = SaveRequestSignal::new();
pub static SAVE_COMPLETION_SIGNAL: SaveCompletionSignal = SaveCompletionSignal::new();
