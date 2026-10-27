//! UI presentation state snapshot and frame update requirements.

use crate::domain::{
    ButtonPressState, ChargeState, DateTime, DeviceOrientation, ImuSample, ImuStatus, MusicScene,
    RtcError, RtcStatus, ViewMode, VoltageValidity,
};
use crate::gesture::GestureEvent;
use crate::pet::PetFrame;
use crate::protocol::{PresenceState, RadioStatus};
use crate::storage::SaveStatus;
use embassy_time::Instant;

#[derive(Clone)]
pub struct UiState {
    pub mode: ViewMode,
    pub datetime: DateTime,
    pub rtc_status: RtcStatus,
    pub save_status: SaveStatus,
    pub v_rail_mv: u16,
    pub measured_v_rail_mv: u16,
    pub voltage_validity: VoltageValidity,
    pub charge_state: ChargeState,
    pub boot_press_state: ButtonPressState,
    pub imu_sample: ImuSample,
    pub imu_status: ImuStatus,
    pub heading_deg: f32,
    pub radio_status: RadioStatus,
    pub presence: PresenceState,
    pub ble_rssi: i8,
    pub last_gesture: Option<GestureEvent>,
    pub orientation: DeviceOrientation,
    pub backlight_pct: u8,
    pub pet: PetFrame,
    pub music: MusicScene,
}

impl core::fmt::Debug for UiState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "UiState {{ mode: {:?}, rtc: {:?}, v_rail: {}mV, pet: {:?} }}",
            self.mode, self.rtc_status, self.v_rail_mv, self.pet
        )
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            mode: ViewMode::Pet,
            datetime: DateTime::default(),
            rtc_status: RtcStatus::NeedsSetting(RtcError::NotValidated),
            save_status: SaveStatus::Clean { seq: 0 },
            v_rail_mv: 4180,
            measured_v_rail_mv: 0,
            voltage_validity: VoltageValidity::ConversionFailed,
            charge_state: ChargeState::StandbyOrDone,
            boot_press_state: ButtonPressState::Released,
            imu_sample: ImuSample {
                ax_mg: 0,
                ay_mg: 0,
                az_mg: 1000,
                gx_mdps: 0,
                gy_mdps: 0,
                gz_mdps: 0,
            },
            imu_status: ImuStatus::default(),
            heading_deg: 0.0,
            radio_status: RadioStatus::Active,
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
        self.mode == ViewMode::Pet && !self.pet.is_sleeping()
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
