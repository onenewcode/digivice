//! Gesture and Physical Interaction Engine for ESP32-C6-LCD-1.28

pub mod detector;
pub mod filter;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureEvent {
    /// Single tap on device/desk (Dismiss alert / wake screen / snooze)
    SingleTap,
    /// Double tap on device/desk (Approve AI command / lock radar snapshot)
    DoubleTap,
    /// Tilt device left by ~30° (Switch to Island View)
    TiltLeft,
    /// Tilt device right by ~30° (Switch to Radar View)
    TiltRight,
    /// Screen turned face down onto desk (DND / Sleep mode)
    FaceDownEnter,
    /// Screen restored face up from desk (Exit DND / Restore brightness)
    FaceDownExit,
    /// Desk heading rotation update in degrees (0..360) for compass-locked radar
    HeadingUpdate(u16),
    /// BOOT key short press (Switch mode Island <-> Radar)
    KeyShortPress,
    /// BOOT key double click (Trigger manual snapshot / bind BLE device)
    KeyDoubleClick,
    /// BOOT key long press >= 1.5s (Recalibrate IMU zero bias)
    KeyLongPress,
}

pub type GestureChannel = Channel<CriticalSectionRawMutex, GestureEvent, 16>;

pub static GESTURE_CHANNEL: GestureChannel = Channel::new();
