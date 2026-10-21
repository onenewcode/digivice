//! Circular UI Rendering Engine for Waveshare 240x240 Round LCD

pub mod island_view;
pub mod pet_view;
pub mod selftest_view;

use crate::board::{ChargeState, DateTime, ImuSample, ImuStatus, RtcStatus};
use crate::gesture::GestureEvent;
use crate::proto::{ActionNotification, HostTelemetry};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

pub const CENTER_X: i32 = 120;
pub const CENTER_Y: i32 = 120;
pub const RADIUS: i32 = 118;

// Dark Geek High-Contrast RGB565 Theme Palette
pub const COLOR_BG: Rgb565 = Rgb565::new(1, 2, 2); // #080C14 Deep dark space
pub const COLOR_CAPSULE_BG: Rgb565 = Rgb565::new(2, 8, 6); // #162032
pub const COLOR_CAPSULE_BORDER: Rgb565 = Rgb565::new(7, 47, 31); // #38BDF8 Cyan blue
pub const COLOR_GREEN: Rgb565 = Rgb565::new(2, 46, 16); // #10B981 Emerald green
pub const COLOR_CYAN: Rgb565 = Rgb565::new(0, 45, 26); // #06B6D4 Cyan
pub const COLOR_AMBER: Rgb565 = Rgb565::new(30, 39, 1); // #F59E0B Amber orange
pub const COLOR_RED: Rgb565 = Rgb565::new(29, 17, 8); // #EF4444 Coral red
pub const COLOR_WHITE: Rgb565 = Rgb565::WHITE;
pub const COLOR_GRAY: Rgb565 = Rgb565::new(12, 25, 12); // Medium gray
pub const COLOR_DARK_GRAY: Rgb565 = Rgb565::new(6, 12, 6); // Dim grid lines

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Island,
    Pet,
    SelfTest,
}

impl From<ViewMode> for u8 {
    fn from(mode: ViewMode) -> Self {
        match mode {
            ViewMode::Island => 0,
            ViewMode::SelfTest => 2,
            ViewMode::Pet => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidViewMode(pub u8);

impl core::fmt::Display for InvalidViewMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid ViewMode tag: {}", self.0)
    }
}

impl core::error::Error for InvalidViewMode {}

impl TryFrom<u8> for ViewMode {
    type Error = InvalidViewMode;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0 => Ok(ViewMode::Island),
            1 => Ok(ViewMode::Island), // Legacy fallback for StandbyClock tag
            2 => Ok(ViewMode::SelfTest),
            3 => Ok(ViewMode::Pet),
            other => Err(InvalidViewMode(other)),
        }
    }
}

impl ViewMode {
    /// Navigate to the view mode to the left (Pet -> Island -> SelfTest).
    pub const fn prev(self) -> Self {
        match self {
            Self::Pet => Self::Island,
            Self::Island => Self::SelfTest,
            Self::SelfTest => Self::SelfTest,
        }
    }

    /// Navigate to the view mode to the right (SelfTest -> Island -> Pet).
    pub const fn next(self) -> Self {
        match self {
            Self::SelfTest => Self::Island,
            Self::Island => Self::Pet,
            Self::Pet => Self::Pet,
        }
    }

    /// Return the next view mode in circular sequence.
    pub const fn cycle(self) -> Self {
        match self {
            Self::SelfTest => Self::Island,
            Self::Island => Self::Pet,
            Self::Pet => Self::SelfTest,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PetMood {
    #[default]
    Normal,
    Happy,
    Focused,
    Dizzy,
    Sleepy,
    Sleeping,
    Shocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PupilShape {
    #[default]
    Round,
    CatSlit,
    Heart,
    Star,
    Dizzy,
    Lightning,
    Cross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetUiState {
    pub mood: PetMood,
    pub pupil_shape: PupilShape,
    pub gaze_x: i8,
    pub gaze_y: i8,
    pub blink_pct: u8,
    pub happy_lift_pct: u8,
    pub anim_tick: u16,
    pub energy: u8,
    pub affinity: u16,
}

impl Default for PetUiState {
    fn default() -> Self {
        Self {
            mood: PetMood::Normal,
            pupil_shape: PupilShape::Round,
            gaze_x: 0,
            gaze_y: 0,
            blink_pct: 100,
            happy_lift_pct: 0,
            anim_tick: 0,
            energy: 100,
            affinity: 0,
        }
    }
}

pub use crate::board::ButtonPressState;
pub use crate::proto::PresenceState;
pub use crate::radio::BleBlip;

/// Strongly-typed physical orientation of the device to eliminate boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceOrientation {
    #[default]
    FaceUp,
    FaceDown,
}

impl DeviceOrientation {
    #[inline]
    pub const fn is_facedown(&self) -> bool {
        matches!(self, Self::FaceDown)
    }
}

impl From<bool> for DeviceOrientation {
    #[inline]
    fn from(facedown: bool) -> Self {
        if facedown {
            Self::FaceDown
        } else {
            Self::FaceUp
        }
    }
}

/// Strongly-typed host connection state to eliminate boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostConnectionState {
    #[default]
    Disconnected,
    Connected,
}

impl HostConnectionState {
    #[inline]
    pub const fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

impl From<bool> for HostConnectionState {
    #[inline]
    fn from(connected: bool) -> Self {
        if connected {
            Self::Connected
        } else {
            Self::Disconnected
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub mode: ViewMode,
    pub telemetry: HostTelemetry,
    pub action: ActionNotification,
    pub datetime: DateTime,
    pub rtc_status: RtcStatus,
    pub v_rail_mv: u16,
    pub charge_state: ChargeState,
    pub boot_press_state: ButtonPressState,
    pub imu_sample: ImuSample,
    pub imu_status: ImuStatus,
    pub heading_deg: f32,
    pub presence: PresenceState,
    pub ble_rssi: i8,
    pub last_gesture: Option<GestureEvent>,
    pub orientation: DeviceOrientation,
    pub host_connection: HostConnectionState,
    pub backlight_pct: u8,
    pub pet: PetUiState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            mode: ViewMode::Island,
            telemetry: HostTelemetry {
                cpu_pct: 35,
                ram_pct: 62,
                rx_kbps: 840,
                tx_kbps: 120,
                cpu_temp: 48,
            },
            action: ActionNotification::Idle,
            datetime: DateTime::default(),
            rtc_status: RtcStatus::FallbackSoftware,
            v_rail_mv: 4180,
            charge_state: ChargeState::StandbyOrDone,
            boot_press_state: ButtonPressState::Released,
            imu_sample: ImuSample {
                ax_mg: 0,
                ay_mg: 0,
                az_mg: 1000,
                gx_mdps: 0,
                gy_mdps: 0,
                gz_mdps: 0,
                temp_c: 26,
            },
            imu_status: ImuStatus::HardwarePresent,
            heading_deg: 0.0,
            presence: PresenceState::Near,
            ble_rssi: -58,
            last_gesture: None,
            orientation: DeviceOrientation::FaceUp,
            host_connection: HostConnectionState::Connected,
            backlight_pct: 80,
            pet: PetUiState::default(),
        }
    }
}

impl UiState {
    /// Check whether high-framerate fluid animation loop (~33 FPS) is actively required.
    #[inline]
    pub fn requires_fluid_animation(&self) -> bool {
        self.action.state() == crate::proto::ActionState::WaitConfirm || self.mode == ViewMode::Pet
    }

    #[inline]
    pub fn is_boot_pressed(&self) -> bool {
        self.boot_press_state.is_pressed()
    }

    #[inline]
    pub fn is_ble_near(&self) -> bool {
        matches!(self.presence, PresenceState::Near)
    }

    #[inline]
    pub fn is_facedown(&self) -> bool {
        self.orientation.is_facedown()
    }

    #[inline]
    pub fn is_host_connected(&self) -> bool {
        self.host_connection.is_connected()
    }
}
