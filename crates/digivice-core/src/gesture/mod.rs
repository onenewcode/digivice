//! Gesture and Physical Interaction Engine for ESP32-C6-LCD-1.28

pub mod delivery;
pub mod detector;
pub mod filter;

pub use delivery::{GestureChannel, OrientationSignal, publish_gesture};
pub use detector::GestureDetector;
pub use filter::{Attitude, ImuFilter, fast_atan2_deg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureEvent {
    /// Single tap on device/desk (pet mascot / play-pause media)
    SingleTap,
    /// Double tap on device/desk (feed meat / trigger special attack)
    DoubleTap,
    /// Tilt device left by ~18° (previous track)
    TiltLeft,
    /// Tilt device right by ~18° (next track)
    TiltRight,
    /// Screen turned face down onto desk (DND / Sleep mode)
    FaceDownEnter,
    /// Screen restored face up from desk (Exit DND / Restore brightness)
    FaceDownExit,
    /// Desk heading rotation update in degrees (0..360)
    HeadingUpdate(u16),
    /// Physical shake/oscillation (Pendulum training / Digimon evolution)
    Shake,
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
            Self::Shake => "Shake",
        }
    }
}

impl core::fmt::Display for GestureEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}
