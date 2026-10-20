//! Mode B: 2.4GHz Polar Situational Wireless Radar UI View

use super::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_CYAN, COLOR_DARK_GRAY, COLOR_GREEN, COLOR_RED,
    COLOR_WHITE, UiState,
};
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;
use libm::{cosf, sinf};

use core::f32::consts::PI;

pub fn render_radar<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let heading_rad = state.heading_deg * PI / 180.0;

    // 1. Concentric Distance Rings: -40 dBm, -65 dBm, -85 dBm
    let ring_style = PrimitiveStyle::with_stroke(COLOR_DARK_GRAY, 1);
    Circle::new(Point::new(CENTER_X - 36, CENTER_Y - 36), 72)
        .into_styled(ring_style)
        .draw(target)
        .ok();
    Circle::new(Point::new(CENTER_X - 72, CENTER_Y - 72), 144)
        .into_styled(ring_style)
        .draw(target)
        .ok();
    Circle::new(Point::new(CENTER_X - 108, CENTER_Y - 108), 216)
        .into_styled(ring_style)
        .draw(target)
        .ok();

    // 2. Rotating Crosshairs & Compass Ticks (N, E, S, W)
    render_compass(target, heading_rad);

    // 3. Rotating Sweep Line (余晖扫描线)
    let sweep_rad = (state.radar_sweep_angle as f32) * PI / 180.0;
    let sweep_x = CENTER_X + (cosf(sweep_rad) * 108.0) as i32;
    let sweep_y = CENTER_Y + (sinf(sweep_rad) * 108.0) as i32;
    Line::new(Point::new(CENTER_X, CENTER_Y), Point::new(sweep_x, sweep_y))
        .into_styled(PrimitiveStyle::with_stroke(COLOR_GREEN, 1))
        .draw(target)
        .ok();

    // 4. Wi-Fi AP Nodes (Cyan squares)
    for ap in &state.wifi_blips {
        // Channel 1..13 mapped around the circle minus heading
        let ch_angle_deg =
            ((ap.channel.saturating_sub(1) as f32) * (360.0 / 13.0)) - state.heading_deg;
        let rad = ch_angle_deg * PI / 180.0;
        let r = rssi_to_radius(ap.rssi);
        let bx = CENTER_X + (cosf(rad) * r) as i32;
        let by = CENTER_Y + (sinf(rad) * r) as i32;

        Rectangle::new(Point::new(bx - 3, by - 3), Size::new(6, 6))
            .into_styled(PrimitiveStyle::with_fill(COLOR_CYAN))
            .draw(target)
            .ok();
    }

    // 5. BLE Active Beacons (Amber circles)
    for ble in &state.ble_blips {
        let ble_angle_deg = ((ble.hash as f32) % 360.0) - state.heading_deg;
        let rad = ble_angle_deg * PI / 180.0;
        let r = rssi_to_radius(ble.rssi);
        let bx = CENTER_X + (cosf(rad) * r) as i32;
        let by = CENTER_Y + (sinf(rad) * r) as i32;

        let color = if ble.is_target {
            COLOR_GREEN
        } else {
            COLOR_AMBER
        };
        Circle::new(Point::new(bx - 3, by - 3), 6)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)
            .ok();
    }

    // 6. Bottom 13-Channel Congestion Histogram Bars (Y: 188..212)
    render_spectrum_bars(target, &state.wifi_congestion);

    // 7. Top Status Bar: AP and BLE Counts + Heading
    let mut stats = String::<32>::new();
    let _ = write!(
        stats,
        "AP:{} BLE:{} HDG:{}",
        state.wifi_blips.len(),
        state.ble_blips.len(),
        state.heading_deg
    );
    Text::new(
        &stats,
        Point::new(68, 30),
        MonoTextStyle::new(&FONT_6X10, COLOR_WHITE),
    )
    .draw(target)
    .ok();
}

fn render_compass<D: DrawTarget<Color = Rgb565>>(target: &mut D, heading_rad: f32) {
    let axis_style = PrimitiveStyle::with_stroke(COLOR_DARK_GRAY, 1);
    let n_rad = -PI / 2.0 - heading_rad;
    let s_rad = PI / 2.0 - heading_rad;
    let e_rad = 0.0 - heading_rad;
    let w_rad = PI - heading_rad;

    // Crosshairs
    Line::new(
        Point::new(
            CENTER_X + (cosf(n_rad) * 105.0) as i32,
            CENTER_Y + (sinf(n_rad) * 105.0) as i32,
        ),
        Point::new(
            CENTER_X + (cosf(s_rad) * 105.0) as i32,
            CENTER_Y + (sinf(s_rad) * 105.0) as i32,
        ),
    )
    .into_styled(axis_style)
    .draw(target)
    .ok();

    Line::new(
        Point::new(
            CENTER_X + (cosf(e_rad) * 105.0) as i32,
            CENTER_Y + (sinf(e_rad) * 105.0) as i32,
        ),
        Point::new(
            CENTER_X + (cosf(w_rad) * 105.0) as i32,
            CENTER_Y + (sinf(w_rad) * 105.0) as i32,
        ),
    )
    .into_styled(axis_style)
    .draw(target)
    .ok();

    // North marker
    let nx = CENTER_X + (cosf(n_rad) * 112.0) as i32;
    let ny = CENTER_Y + (sinf(n_rad) * 112.0) as i32;
    Text::new(
        "N",
        Point::new(nx - 3, ny + 4),
        MonoTextStyle::new(&FONT_6X10, COLOR_RED),
    )
    .draw(target)
    .ok();
}

fn render_spectrum_bars<D: DrawTarget<Color = Rgb565>>(target: &mut D, congestion: &[u8; 13]) {
    // 13 channels from X: 48 to 192 (11px per channel: 8px bar + 3px gap)
    let base_y: i32 = 210;
    let max_height: i32 = 22;

    for (ch, &score) in congestion.iter().enumerate().take(13) {
        let x = 48 + (ch as i32) * 11;
        let h = ((score as i32) * max_height) / 100;
        let bar_color = if score > 70 {
            COLOR_RED
        } else if score > 35 {
            COLOR_AMBER
        } else {
            COLOR_GREEN
        };

        if h > 0 {
            Rectangle::new(Point::new(x, base_y - h), Size::new(8, h as u32))
                .into_styled(PrimitiveStyle::with_fill(bar_color))
                .draw(target)
                .ok();
        } else {
            // 1px baseline indicator
            Rectangle::new(Point::new(x, base_y - 1), Size::new(8, 1))
                .into_styled(PrimitiveStyle::with_fill(COLOR_DARK_GRAY))
                .draw(target)
                .ok();
        }
    }
}

/// Map RSSI (-40 dBm down to -90 dBm) to radius (20px to 105px)
fn rssi_to_radius(rssi: i8) -> f32 {
    let clamped = (rssi.max(-90) as f32).min(-40.0);
    // -40 dBm -> 22px (center), -90 dBm -> 104px (outer edge)
    22.0 + ((-40.0 - clamped) / 50.0) * 82.0
}
