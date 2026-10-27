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

#[inline(never)]
fn draw_label<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    text: &str,
    point: Point,
    color: Rgb565,
) {
    let style = MonoTextStyle::new(&FONT_6X10, color);
    Text::new(text, point, style).draw(target).ok();
}

#[inline(never)]
fn render_selftest_frame<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
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
}

#[inline(never)]
fn render_imu_telemetry<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let mut line_buf = String::<36>::new();

    // Line 1: LCD/PWM
    let _ = write!(line_buf, "LCD/PWM: OK (BL {}%)", state.backlight_pct);
    draw_label(target, &line_buf, Point::new(32, 62), COLOR_GREEN);

    // Line 2: IMU WHO_AM_I / Status
    match state.imu_status {
        ImuStatus::HardwarePresent => {
            draw_label(
                target,
                "IMU 0x6B: OK (WHO=0x05)",
                Point::new(32, 80),
                COLOR_GREEN,
            );
        }
        ImuStatus::Unavailable(err) => {
            line_buf.clear();
            let _ = write!(line_buf, "IMU 0x6B: ERR ({})", err);
            draw_label(target, &line_buf, Point::new(32, 80), COLOR_AMBER);
        }
    }

    // Line 3: ACC (mg)
    line_buf.clear();
    let _ = write!(
        line_buf,
        "ACC: X:{:+05} Y:{:+05}",
        state.imu_sample.ax_mg, state.imu_sample.ay_mg
    );
    draw_label(target, &line_buf, Point::new(26, 98), COLOR_WHITE);

    line_buf.clear();
    let _ = write!(line_buf, "     Z:{:+05} (mg)", state.imu_sample.az_mg);
    draw_label(target, &line_buf, Point::new(26, 114), COLOR_WHITE);

    // Line 4: GYR (dps)
    line_buf.clear();
    let _ = write!(
        line_buf,
        "GYR: Z:{:+04} dps HDG:{:03}",
        state.imu_sample.gz_mdps / 1000,
        state.heading_deg as u16
    );
    draw_label(target, &line_buf, Point::new(26, 132), COLOR_WHITE);
}

#[inline(never)]
fn render_system_telemetry<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let mut line_buf = String::<36>::new();

    // Line 5: RTC 0x51
    if state.rtc_status == RtcStatus::HardwarePresent {
        let _ = write!(
            line_buf,
            "RTC 0x51: {:02}:{:02}:{:02}",
            state.datetime.hour(),
            state.datetime.minute(),
            state.datetime.second()
        );
        draw_label(target, &line_buf, Point::new(26, 150), COLOR_GREEN);
    } else {
        let label = match state.rtc_status {
            RtcStatus::FallbackSoftware(_) => "RTC: SOFT (ANCHORED)",
            RtcStatus::NeedsSetting(_) => "RTC: SET TIME (UNTRUSTED)",
            RtcStatus::HardwarePresent => "RTC: HW OK",
        };
        draw_label(target, label, Point::new(26, 150), COLOR_AMBER);
    }

    // Line 6: PWR Rail & Charging State
    line_buf.clear();
    let _ = write!(
        line_buf,
        "PWR:{}mV RAW:{} {:?}",
        state.v_rail_mv, state.measured_v_rail_mv, state.voltage_validity
    );
    draw_label(target, &line_buf, Point::new(26, 168), COLOR_WHITE);

    // Line 7: BOOT Strapping Pin & Last Gesture
    line_buf.clear();
    let boot = if state.is_boot_pressed() {
        "PRSD"
    } else {
        "REL "
    };
    let evt = state.last_gesture.map(|g| g.name()).unwrap_or("Ready");
    let _ = write!(line_buf, "BOOT:{} EVT:{}", boot, evt);
    draw_label(target, &line_buf, Point::new(26, 186), COLOR_CYAN);
}

pub fn render_selftest<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    render_selftest_frame(target, state);
    draw_label(target, "HARDWARE SELF-TEST", Point::new(65, 42), COLOR_CYAN);
    render_imu_telemetry(target, state);
    render_system_telemetry(target, state);
}
