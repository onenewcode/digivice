//! Standby RTC Mechanical Dial Clock UI View

use super::{CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_GRAY, COLOR_GREEN, COLOR_WHITE, UiState};
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle};
use embedded_graphics::text::Text;
use heapless::String;
use libm::{cosf, sinf};

use core::f32::consts::PI;

pub fn render_clock<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    // 1. Dial Hour Markers (12 ticks around R=104)
    for hour in 0..12 {
        let angle_rad = ((hour as f32) * 30.0 - 90.0) * PI / 180.0;
        let is_cardinal = hour % 3 == 0;
        let r_inner = if is_cardinal { 92.0 } else { 98.0 };
        let r_outer = 104.0;
        let p1 = Point::new(
            CENTER_X + (cosf(angle_rad) * r_inner) as i32,
            CENTER_Y + (sinf(angle_rad) * r_inner) as i32,
        );
        let p2 = Point::new(
            CENTER_X + (cosf(angle_rad) * r_outer) as i32,
            CENTER_Y + (sinf(angle_rad) * r_outer) as i32,
        );
        let color = if is_cardinal { COLOR_WHITE } else { COLOR_GRAY };
        let width = if is_cardinal { 2 } else { 1 };
        Line::new(p1, p2)
            .into_styled(PrimitiveStyle::with_stroke(color, width))
            .draw(target)
            .ok();
    }

    // 2. Hour Hand (Length 48px, 3px wide)
    let hour_val = (state.datetime.hour % 12) as f32 + (state.datetime.minute as f32) / 60.0;
    let hour_rad = (hour_val * 30.0 - 90.0) * PI / 180.0;
    let hx = CENTER_X + (cosf(hour_rad) * 48.0) as i32;
    let hy = CENTER_Y + (sinf(hour_rad) * 48.0) as i32;
    Line::new(Point::new(CENTER_X, CENTER_Y), Point::new(hx, hy))
        .into_styled(PrimitiveStyle::with_stroke(COLOR_WHITE, 3))
        .draw(target)
        .ok();

    // 3. Minute Hand (Length 74px, 2px wide)
    let min_val = state.datetime.minute as f32 + (state.datetime.second as f32) / 60.0;
    let min_rad = (min_val * 6.0 - 90.0) * PI / 180.0;
    let mx = CENTER_X + (cosf(min_rad) * 74.0) as i32;
    let my = CENTER_Y + (sinf(min_rad) * 74.0) as i32;
    Line::new(Point::new(CENTER_X, CENTER_Y), Point::new(mx, my))
        .into_styled(PrimitiveStyle::with_stroke(COLOR_WHITE, 2))
        .draw(target)
        .ok();

    // 4. Second Hand (Length 86px, Amber, 1px)
    let sec_rad = ((state.datetime.second as f32) * 6.0 - 90.0) * PI / 180.0;
    let sx = CENTER_X + (cosf(sec_rad) * 86.0) as i32;
    let sy = CENTER_Y + (sinf(sec_rad) * 86.0) as i32;
    // Tail extension
    let tail_x = CENTER_X - (cosf(sec_rad) * 16.0) as i32;
    let tail_y = CENTER_Y - (sinf(sec_rad) * 16.0) as i32;
    Line::new(Point::new(tail_x, tail_y), Point::new(sx, sy))
        .into_styled(PrimitiveStyle::with_stroke(COLOR_AMBER, 1))
        .draw(target)
        .ok();

    // 5. Center Pin
    Circle::new(Point::new(CENTER_X - 4, CENTER_Y - 4), 8)
        .into_styled(PrimitiveStyle::with_fill(COLOR_WHITE))
        .draw(target)
        .ok();
    Circle::new(Point::new(CENTER_X - 2, CENTER_Y - 2), 4)
        .into_styled(PrimitiveStyle::with_fill(COLOR_AMBER))
        .draw(target)
        .ok();

    // 6. Sub-dial / Date & Voltage at 6 o'clock (Y: 154..174)
    let mut date_str = String::<16>::new();
    let _ = write!(
        date_str,
        "{:02}/{:02}",
        state.datetime.month, state.datetime.day
    );
    Text::new(
        &date_str,
        Point::new(CENTER_X - 16, 162),
        MonoTextStyle::new(&FONT_6X10, COLOR_GRAY),
    )
    .draw(target)
    .ok();

    let mut v_str = String::<16>::new();
    let volts = state.v_rail_mv / 1000;
    let centivolts = (state.v_rail_mv % 1000) / 10;
    let _ = write!(v_str, "{}.{:02}V", volts, centivolts);
    Text::new(
        &v_str,
        Point::new(CENTER_X - 16, 178),
        MonoTextStyle::new(&FONT_6X10, COLOR_GREEN),
    )
    .draw(target)
    .ok();
}
