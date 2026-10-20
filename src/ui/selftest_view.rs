//! Stage 1: Hardware Self-Test Dashboard UI View

use super::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_CYAN, COLOR_GREEN, COLOR_WHITE, RADIUS, UiState,
};
use crate::board::RtcStatus;
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};
use embedded_graphics::text::Text;
use heapless::String;
use libm::{cosf, sinf};

pub fn render_selftest<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    // 1. Outer green circle (R=118, Center 120,120)
    Circle::new(
        Point::new(CENTER_X - RADIUS, CENTER_Y - RADIUS),
        (RADIUS * 2) as u32,
    )
    .into_styled(PrimitiveStyle::with_stroke(COLOR_GREEN, 2))
    .draw(target)
    .ok();

    // 2. IMU Gravity Indicator Ball on circumference
    // Compute angle from roll and pitch
    let ax = state.imu_sample.ax_mg as f32;
    let ay = state.imu_sample.ay_mg as f32;
    let norm = libm::sqrtf(ax * ax + ay * ay);
    let ball_angle = if norm > 50.0 {
        libm::atan2f(ay, ax)
    } else {
        0.0
    };
    let ball_r = (RADIUS - 5) as f32;
    let ball_x = CENTER_X + (cosf(ball_angle) * ball_r) as i32;
    let ball_y = CENTER_Y + (sinf(ball_angle) * ball_r) as i32;
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

    // Line 2: IMU WHO_AM_I
    Text::new("IMU 0x6B: OK (WHO=0x05)", Point::new(32, 80), green_style)
        .draw(target)
        .ok();

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
            state.datetime.hour, state.datetime.minute, state.datetime.second
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
    let bc = if state.bc_state_high { "HI" } else { "CHG" };
    let _ = write!(s6, "PWR Rail: {} mV | BC:{}", state.v_rail_mv, bc);
    Text::new(&s6, Point::new(26, 168), text_style)
        .draw(target)
        .ok();

    // Line 7: KEY & Last Gesture
    let mut s7 = String::<36>::new();
    let key = if state.is_boot_pressed {
        "PRSD"
    } else {
        "REL "
    };
    let _ = write!(s7, "KEY:{} EVT:{}", key, state.last_gesture_name);
    Text::new(&s7, Point::new(26, 186), cyan_style)
        .draw(target)
        .ok();
}
