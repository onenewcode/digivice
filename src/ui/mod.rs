//! Circular UI Rendering Engine for Waveshare 240x240 Round LCD

pub mod clock_view;
pub mod island_view;
pub mod radar_view;
pub mod selftest_view;

use crate::board::{DateTime, ImuSample, RtcStatus};
use crate::proto::{ActionNotification, ActionState, HostTelemetry};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;
use heapless::Vec;

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
    Radar,
    StandbyClock,
    SelfTest,
}

#[derive(Debug, Clone, Copy)]
pub struct BleBlip {
    pub hash: u16,
    pub rssi: i8,
    pub is_target: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct WifiApBlip {
    pub channel: u8,
    pub rssi: i8,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub mode: ViewMode,
    pub telemetry: HostTelemetry,
    pub action: ActionNotification,
    pub datetime: DateTime,
    pub rtc_status: RtcStatus,
    pub v_rail_mv: u16,
    pub bc_state_high: bool,
    pub is_boot_pressed: bool,
    pub imu_sample: ImuSample,
    pub roll_deg: f32,
    pub pitch_deg: f32,
    pub heading_deg: f32,
    pub wifi_congestion: [u8; 13],
    pub ble_near: bool,
    pub ble_rssi: i8,
    pub ble_blips: Vec<BleBlip, 12>,
    pub wifi_blips: Vec<WifiApBlip, 8>,
    pub radar_sweep_angle: u16,
    pub last_gesture_name: &'static str,
    pub is_facedown: bool,
    pub host_connected: bool,
    pub backlight_pct: u8,
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
            action: ActionNotification {
                state: ActionState::Idle,
                source: heapless::String::new(),
                title: heapless::String::new(),
                progress: 0,
            },
            datetime: DateTime::default(),
            rtc_status: RtcStatus::FallbackSoftware,
            v_rail_mv: 4180,
            bc_state_high: true,
            is_boot_pressed: false,
            imu_sample: ImuSample {
                ax_mg: 0,
                ay_mg: 0,
                az_mg: 1000,
                gx_mdps: 0,
                gy_mdps: 0,
                gz_mdps: 0,
                temp_c: 26,
            },
            roll_deg: 0.0,
            pitch_deg: 0.0,
            heading_deg: 0.0,
            wifi_congestion: [15, 30, 10, 5, 20, 75, 40, 10, 5, 20, 85, 30, 10],
            ble_near: true,
            ble_rssi: -58,
            ble_blips: Vec::new(),
            wifi_blips: Vec::new(),
            radar_sweep_angle: 0,
            last_gesture_name: "Ready",
            is_facedown: false,
            host_connected: true,
            backlight_pct: 80,
        }
    }
}
