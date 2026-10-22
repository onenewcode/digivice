//! Gesture and Physical Interaction Engine for ESP32-C6-LCD-1.28

pub mod detector;
pub mod filter;

pub use detector::GestureDetector;
pub use filter::{Attitude, ImuFilter};

pub use crate::ipc::{GESTURE_CHANNEL, GestureChannel};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureEvent {
    /// Single tap on device/desk (Dismiss alert / wake screen / snooze)
    SingleTap,
    /// Double tap on device/desk (feed or toggle local focus)
    DoubleTap,
    /// Tilt device left by ~18° (Navigate to left view mode: Pet -> Island -> SelfTest)
    TiltLeft,
    /// Tilt device right by ~18° (Navigate to right view mode: SelfTest -> Island -> Pet)
    TiltRight,
    /// Screen turned face down onto desk (DND / Sleep mode)
    FaceDownEnter,
    /// Screen restored face up from desk (Exit DND / Restore brightness)
    FaceDownExit,
    /// Desk heading rotation update in degrees (0..360)
    HeadingUpdate(u16),
}

impl GestureEvent {
    /// Return short uppercase/camel representation for UI rendering and logs.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SingleTap => "SingleTap",
            Self::DoubleTap => "DoubleTap",
            Self::TiltLeft => "TiltLeft",
            Self::TiltRight => "TiltRight",
            Self::FaceDownEnter => "FaceDownEnter",
            Self::FaceDownExit => "FaceDownExit",
            Self::HeadingUpdate(_) => "Heading",
        }
    }
}

impl core::fmt::Display for GestureEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}
