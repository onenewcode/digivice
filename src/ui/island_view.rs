//! Mode A: Desktop Dynamic Island UI View

use super::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_CAPSULE_BG, COLOR_CAPSULE_BORDER, COLOR_CYAN,
    COLOR_DARK_GRAY, COLOR_GRAY, COLOR_GREEN, COLOR_RED, COLOR_WHITE, RADIUS, UiState,
};
use crate::proto::ActionState;
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_9X18};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;
use libm::{cosf, sinf};

use core::f32::consts::PI;

pub fn render_island<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    // 1. Outer Left Arc (CPU%) and Right Arc (RAM%)
    render_meter_arcs(target, state.telemetry.cpu_pct, state.telemetry.ram_pct);

    // 2. Dynamic Island Capsule at Top (Y: 20..66)
    render_capsule(target, state);

    // 3. Center Area: Big Time, Date, Telemetry
    render_center_info(target, state);

    // 4. Bottom Dock: BLE Presence & Network Throughput
    render_bottom_dock(target, state);
}

fn render_capsule<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let action = &state.action;
    match action.state {
        ActionState::Idle => {
            // Compact pill at top: 76x22 px (X: 82..158, Y: 24..46)
            let rect = Rectangle::new(Point::new(82, 24), Size::new(76, 22));
            rect.into_styled(PrimitiveStyle::with_fill(COLOR_CAPSULE_BG))
                .draw(target)
                .ok();
            rect.into_styled(PrimitiveStyle::with_stroke(COLOR_CAPSULE_BORDER, 1))
                .draw(target)
                .ok();

            // Status dot + voltage
            Circle::new(Point::new(88, 32), 6)
                .into_styled(PrimitiveStyle::with_fill(if state.host_connected {
                    COLOR_GREEN
                } else {
                    COLOR_GRAY
                }))
                .draw(target)
                .ok();

            let mut v_str = String::<16>::new();
            let volts = state.v_rail_mv / 1000;
            let centivolts = (state.v_rail_mv % 1000) / 10;
            let _ = write!(v_str, " {}.{:02}V", volts, centivolts);
            Text::new(
                &v_str,
                Point::new(100, 39),
                MonoTextStyle::new(&FONT_6X10, COLOR_WHITE),
            )
            .draw(target)
            .ok();
        }
        _ => {
            // Expanded capsule: 176x44 px (X: 32..208, Y: 22..66)
            let border_color = match action.state {
                ActionState::Thinking => COLOR_CYAN,
                ActionState::WaitConfirm => COLOR_AMBER,
                ActionState::Success => COLOR_GREEN,
                ActionState::Error => COLOR_RED,
                ActionState::Idle => COLOR_CAPSULE_BORDER,
            };

            let rect = Rectangle::new(Point::new(32, 22), Size::new(176, 44));
            rect.into_styled(PrimitiveStyle::with_fill(COLOR_CAPSULE_BG))
                .draw(target)
                .ok();
            rect.into_styled(PrimitiveStyle::with_stroke(border_color, 2))
                .draw(target)
                .ok();

            // Line 1: Source + Status badge
            let mut l1 = String::<32>::new();
            let _ = write!(l1, "{} [{}]", action.source, action.state.as_str());
            Text::new(
                &l1,
                Point::new(38, 36),
                MonoTextStyle::new(&FONT_6X10, border_color),
            )
            .draw(target)
            .ok();

            // Line 2: Action Title
            let mut l2 = String::<32>::new();
            if action.title.is_empty() {
                if action.state == ActionState::WaitConfirm {
                    let _ = write!(l2, "Double-tap to approve");
                } else {
                    let _ = write!(l2, "Processing task...");
                }
            } else {
                let _ = write!(l2, "{}", action.title);
            }
            Text::new(
                &l2,
                Point::new(38, 50),
                MonoTextStyle::new(&FONT_6X10, COLOR_WHITE),
            )
            .draw(target)
            .ok();

            // Progress bar at bottom of capsule
            let bar_w = ((action.progress as u32) * 168) / 100;
            Rectangle::new(Point::new(36, 61), Size::new(168, 2))
                .into_styled(PrimitiveStyle::with_fill(COLOR_DARK_GRAY))
                .draw(target)
                .ok();
            if bar_w > 0 {
                Rectangle::new(Point::new(36, 61), Size::new(bar_w, 2))
                    .into_styled(PrimitiveStyle::with_fill(border_color))
                    .draw(target)
                    .ok();
            }
        }
    }
}

fn render_center_info<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    // Big Time: HH:MM:SS at Y: 104
    let mut time_str = String::<16>::new();
    let _ = write!(
        time_str,
        "{:02}:{:02}:{:02}",
        state.datetime.hour, state.datetime.minute, state.datetime.second
    );

    Text::new(
        &time_str,
        Point::new(84, 98),
        MonoTextStyle::new(&FONT_9X18, COLOR_WHITE),
    )
    .draw(target)
    .ok();

    // Date: YYYY-MM-DD
    let mut date_str = String::<16>::new();
    let _ = write!(
        date_str,
        "{}-{:02}-{:02}",
        state.datetime.year, state.datetime.month, state.datetime.day
    );

    Text::new(
        &date_str,
        Point::new(90, 114),
        MonoTextStyle::new(&FONT_6X10, COLOR_GRAY),
    )
    .draw(target)
    .ok();

    // Core Telemetry: CPU / RAM stats
    let mut cpu_str = String::<32>::new();
    let _ = write!(
        cpu_str,
        "CPU {}% ({}C)  RAM {}%",
        state.telemetry.cpu_pct, state.telemetry.cpu_temp, state.telemetry.ram_pct
    );

    Text::new(
        &cpu_str,
        Point::new(54, 134),
        MonoTextStyle::new(&FONT_6X10, COLOR_CYAN),
    )
    .draw(target)
    .ok();

    // Network throughput
    let mut net_str = String::<32>::new();
    let _ = net_str.push_str("RX ");
    format_net_speed(&mut net_str, state.telemetry.rx_kbps);
    let _ = net_str.push_str(" / TX ");
    format_net_speed(&mut net_str, state.telemetry.tx_kbps);

    Text::new(
        &net_str,
        Point::new(58, 150),
        MonoTextStyle::new(&FONT_6X10, COLOR_WHITE),
    )
    .draw(target)
    .ok();
}

fn render_bottom_dock<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    // BLE Presence Badge (Y: 172..192)
    let dot_color = if state.ble_near {
        COLOR_GREEN
    } else {
        COLOR_GRAY
    };
    Circle::new(Point::new(76, 178), 6)
        .into_styled(PrimitiveStyle::with_fill(dot_color))
        .draw(target)
        .ok();

    let mut ble_str = String::<24>::new();
    if state.ble_near {
        let _ = write!(ble_str, "NEAR ({}dB)", state.ble_rssi);
    } else {
        let _ = ble_str.push_str("AWAY (LOCKED)");
    }

    Text::new(
        &ble_str,
        Point::new(88, 185),
        MonoTextStyle::new(
            &FONT_6X10,
            if state.ble_near {
                COLOR_GREEN
            } else {
                COLOR_GRAY
            },
        ),
    )
    .draw(target)
    .ok();
}

/// Render CPU and RAM load meter arcs along circular bezel
fn render_meter_arcs<D: DrawTarget<Color = Rgb565>>(target: &mut D, cpu_pct: u8, ram_pct: u8) {
    let r_outer = RADIUS as f32 - 2.0;
    let r_inner = RADIUS as f32 - 7.0;

    // Left Arc: CPU% from 135° down to 225° (range: 90 degrees)
    // 135° = 2.356 rad, 225° = 3.927 rad
    let cpu_fill_deg = (cpu_pct as f32) * 0.9; // 0..100% -> 0..90°
    let cpu_color = if cpu_pct > 80 { COLOR_RED } else { COLOR_CYAN };
    for deg in 0..90 {
        let angle_deg = 135.0 + (deg as f32);
        let rad = angle_deg * PI / 180.0;
        let c = if (deg as f32) <= cpu_fill_deg {
            cpu_color
        } else {
            COLOR_DARK_GRAY
        };
        let p1 = Point::new(
            CENTER_X + (cosf(rad) * r_inner) as i32,
            CENTER_Y + (sinf(rad) * r_inner) as i32,
        );
        let p2 = Point::new(
            CENTER_X + (cosf(rad) * r_outer) as i32,
            CENTER_Y + (sinf(rad) * r_outer) as i32,
        );
        Line::new(p1, p2)
            .into_styled(PrimitiveStyle::with_stroke(c, 1))
            .draw(target)
            .ok();
    }

    // Right Arc: RAM% from -45° to +45° (range: 90 degrees)
    let ram_fill_deg = (ram_pct as f32) * 0.9;
    let ram_color = if ram_pct > 85 {
        COLOR_AMBER
    } else {
        COLOR_GREEN
    };
    for deg in 0..90 {
        let angle_deg = -45.0 + (deg as f32);
        let rad = angle_deg * PI / 180.0;
        let c = if (deg as f32) <= ram_fill_deg {
            ram_color
        } else {
            COLOR_DARK_GRAY
        };
        let p1 = Point::new(
            CENTER_X + (cosf(rad) * r_inner) as i32,
            CENTER_Y + (sinf(rad) * r_inner) as i32,
        );
        let p2 = Point::new(
            CENTER_X + (cosf(rad) * r_outer) as i32,
            CENTER_Y + (sinf(rad) * r_outer) as i32,
        );
        Line::new(p1, p2)
            .into_styled(PrimitiveStyle::with_stroke(c, 1))
            .draw(target)
            .ok();
    }
}

fn format_net_speed(s: &mut String<32>, kbps: u32) {
    if kbps >= 1024 {
        let mb = kbps / 1024;
        let frac = (kbps % 1024) / 102;
        let _ = write!(s, "{mb}.{frac}M");
    } else {
        let _ = write!(s, "{kbps}K");
    }
}
