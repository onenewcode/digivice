//! Stage 1: Hardware Self-Test Dashboard UI View

use super::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_CYAN, COLOR_GREEN, COLOR_WHITE, RADIUS, UiState,
};
use crate::board::{ImuStatus, RtcStatus};
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};
use embedded_graphics::text::Text;
use heapless::String;

pub fn render_selftest<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    // 1. Outer green circle (R=118, Center 120,120)
    Circle::new(
        Point::new(CENTER_X - RADIUS, CENTER_Y - RADIUS),
        (RADIUS * 2) as u32,
    )
    .into_styled(PrimitiveStyle::with_stroke(COLOR_GREEN, 2))
    .draw(target)
    .ok();

    // 2. IMU Gravity Indicator Ball on circumference (direct proportional projection, Zero-Trig & Zero-FPU)
    let ax = state.imu_sample.ax_mg;
    let ay = state.imu_sample.ay_mg;
    let norm_sq = (ax as i64 * ax as i64 + ay as i64 * ay as i64) as u64;
    let (ball_x, ball_y) = if norm_sq > 2500 {
        let norm = norm_sq.isqrt() as i32;
        let ball_r = RADIUS - 5;
        (
            CENTER_X + (ax * ball_r) / norm,
            CENTER_Y + (ay * ball_r) / norm,
        )
    } else {
        (CENTER_X, CENTER_Y)
    };
    Circle::new(Point::new(ball_x - 4, ball_y - 4), 8)
        .into_styled(PrimitiveStyle::with_fill(COLOR_CYAN))
        .draw(target)
        .ok();

    // 3. Central Telemetry Lines (Y: 48..190)
    let text_style = MonoTextStyle::new(&FONT_6X10, COLOR_WHITE);
    let green_style = MonoTextStyle::new(&FONT_6X10, COLOR_GREEN);
    let amber_style = MonoTextStyle::new(&FONT_6X10, COLOR_AMBER);
    let cyan_style = MonoTextStyle::new(&FONT_6X10, COLOR_CYAN);

    // Title
    Text::new("HARDWARE SELF-TEST", Point::new(65, 42), cyan_style)
        .draw(target)
        .ok();

    // Line 1: LCD/PWM
    let mut s1 = String::<32>::new();
    let _ = write!(s1, "LCD/PWM: OK (BL {}%)", state.backlight_pct);
    Text::new(&s1, Point::new(32, 62), green_style)
        .draw(target)
        .ok();

    // Line 2: IMU WHO_AM_I / Status
    match state.imu_status {
        ImuStatus::HardwarePresent => {
            Text::new("IMU 0x6B: OK (WHO=0x05)", Point::new(32, 80), green_style)
                .draw(target)
                .ok();
        }
        ImuStatus::Unavailable(err) => {
            let mut s2 = String::<36>::new();
            let _ = write!(s2, "IMU 0x6B: ERR ({})", err);
            Text::new(&s2, Point::new(32, 80), amber_style)
                .draw(target)
                .ok();
        }
    }

    // Line 3: ACC (mg)
    let mut s3 = String::<36>::new();
    let _ = write!(
        s3,
        "ACC: X:{:+05} Y:{:+05}",
        state.imu_sample.ax_mg, state.imu_sample.ay_mg
    );
    Text::new(&s3, Point::new(26, 98), text_style)
        .draw(target)
        .ok();

    let mut s3b = String::<36>::new();
    let _ = write!(s3b, "     Z:{:+05} (mg)", state.imu_sample.az_mg);
    Text::new(&s3b, Point::new(26, 114), text_style)
        .draw(target)
        .ok();

    // Line 4: GYR (dps)
    let mut s4 = String::<36>::new();
    let _ = write!(
        s4,
        "GYR: Z:{:+04} dps HDG:{}",
        state.imu_sample.gz_mdps / 1000,
        state.heading_deg
    );
    Text::new(&s4, Point::new(26, 132), text_style)
        .draw(target)
        .ok();

    // Line 5: RTC 0x51
    let mut s5 = String::<36>::new();
    if state.rtc_status == RtcStatus::HardwarePresent {
        let _ = write!(
            s5,
            "RTC 0x51: {:02}:{:02}:{:02}",
            state.datetime.hour(),
            state.datetime.minute(),
            state.datetime.second()
        );
        Text::new(&s5, Point::new(26, 150), green_style)
            .draw(target)
            .ok();
    } else {
        let _ = write!(s5, "RTC 0x51: FALLBACK (SOFT)");
        Text::new(&s5, Point::new(26, 150), amber_style)
            .draw(target)
            .ok();
    }

    // Line 6: PWR Rail & Charging State
    let mut s6 = String::<36>::new();
    let bc = state.charge_state.as_str();
    let _ = write!(s6, "PWR Rail: {} mV | BC:{}", state.v_rail_mv, bc);
    Text::new(&s6, Point::new(26, 168), text_style)
        .draw(target)
        .ok();

    // Line 7: BOOT Strapping Pin & Last Gesture
    let mut s7 = String::<36>::new();
    let boot = if state.is_boot_pressed() {
        "PRSD"
    } else {
        "REL "
    };
    let evt = state.last_gesture.map(|g| g.name()).unwrap_or("Ready");
    let _ = write!(s7, "BOOT:{} EVT:{}", boot, evt);
    Text::new(&s7, Point::new(26, 186), cyan_style)
        .draw(target)
        .ok();
}
