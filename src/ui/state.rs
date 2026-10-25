//! UI presentation state snapshot and frame update requirements.

use crate::board::{ButtonPressState, ChargeState, DateTime, ImuSample, ImuStatus, RtcStatus};
use crate::domain::{DeviceOrientation, MusicScene, ViewMode};
use crate::gesture::GestureEvent;
use crate::pet::PetFrame;
use crate::radio::PresenceState;
use embassy_time::Instant;

#[derive(Debug, Clone)]
pub struct UiState {
    pub mode: ViewMode,
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
    pub backlight_pct: u8,
    pub pet: PetFrame,
    pub music: MusicScene,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            mode: ViewMode::Pet,
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
            backlight_pct: 80,
            pet: PetFrame::default(),
            music: MusicScene::inactive(Instant::now()),
        }
    }
}

impl UiState {
    /// Check whether high-framerate fluid animation loop (~33 FPS) is actively required.
    #[inline]
    pub fn requires_fluid_animation(&self) -> bool {
        self.mode == ViewMode::Pet
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
}
