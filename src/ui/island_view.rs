//! Mode A: Desktop Dynamic Island UI View

use super::{
    COLOR_AMBER, COLOR_CAPSULE_BG, COLOR_CAPSULE_BORDER, COLOR_CYAN, COLOR_DARK_GRAY, COLOR_GRAY,
    COLOR_GREEN, COLOR_RED, COLOR_WHITE, UiState,
};
use crate::proto::{ActionNotification, ActionState};
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_9X18};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;

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
    match action {
        ActionNotification::Idle => {
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
                .into_styled(PrimitiveStyle::with_fill(if state.is_host_connected() {
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
        active => {
            // Expanded capsule: 176x44 px (X: 32..208, Y: 22..66)
            let action_state = active.state();
            let border_color = match action_state {
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
            let _ = write!(l1, "{} [{}]", active.source(), action_state.as_str());
            Text::new(
                &l1,
                Point::new(38, 36),
                MonoTextStyle::new(&FONT_6X10, border_color),
            )
            .draw(target)
            .ok();

            // Line 2: Action Title
            let mut l2 = String::<32>::new();
            if active.title().is_empty() {
                if action_state == ActionState::WaitConfirm {
                    let _ = write!(l2, "Double-tap to approve");
                } else {
                    let _ = write!(l2, "Processing task...");
                }
            } else {
                let _ = write!(l2, "{}", active.title());
            }
            Text::new(
                &l2,
                Point::new(38, 50),
                MonoTextStyle::new(&FONT_6X10, COLOR_WHITE),
            )
            .draw(target)
            .ok();

            // Progress bar at bottom of capsule
            let bar_w = ((active.progress() as u32) * 168) / 100;
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
        state.datetime.hour(),
        state.datetime.minute(),
        state.datetime.second()
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
        state.datetime.year(),
        state.datetime.month(),
        state.datetime.day()
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
    let is_near = state.is_ble_near();
    let dot_color = if is_near { COLOR_GREEN } else { COLOR_GRAY };
    Circle::new(Point::new(76, 178), 6)
        .into_styled(PrimitiveStyle::with_fill(dot_color))
        .draw(target)
        .ok();

    let mut ble_str = String::<24>::new();
    if is_near {
        let _ = write!(ble_str, "NEAR ({}dB)", state.ble_rssi);
    } else {
        let _ = ble_str.push_str("AWAY (LOCKED)");
    }

    Text::new(
        &ble_str,
        Point::new(88, 185),
        MonoTextStyle::new(&FONT_6X10, if is_near { COLOR_GREEN } else { COLOR_GRAY }),
    )
    .draw(target)
    .ok();
}

/// Precomputed tick coordinates for CPU meter arc (135° to 224°, 90 ticks)
const CPU_ARC_TICKS: [(Point, Point); 90] = [
    (Point::new(42, 198), Point::new(38, 202)),
    (Point::new(40, 197), Point::new(37, 201)),
    (Point::new(39, 196), Point::new(35, 199)),
    (Point::new(38, 194), Point::new(34, 198)),
    (Point::new(36, 193), Point::new(32, 196)),
    (Point::new(35, 191), Point::new(31, 195)),
    (Point::new(34, 190), Point::new(30, 193)),
    (Point::new(33, 188), Point::new(29, 191)),
    (Point::new(31, 187), Point::new(27, 190)),
    (Point::new(30, 185), Point::new(26, 188)),
    (Point::new(29, 184), Point::new(25, 187)),
    (Point::new(28, 182), Point::new(24, 185)),
    (Point::new(27, 180), Point::new(23, 183)),
    (Point::new(26, 179), Point::new(22, 181)),
    (Point::new(25, 177), Point::new(21, 180)),
    (Point::new(24, 176), Point::new(20, 178)),
    (Point::new(23, 174), Point::new(19, 176)),
    (Point::new(22, 172), Point::new(18, 174)),
    (Point::new(21, 170), Point::new(17, 173)),
    (Point::new(20, 169), Point::new(16, 171)),
    (Point::new(19, 167), Point::new(15, 169)),
    (Point::new(19, 165), Point::new(14, 167)),
    (Point::new(18, 163), Point::new(13, 165)),
    (Point::new(17, 162), Point::new(12, 163)),
    (Point::new(16, 160), Point::new(12, 162)),
    (Point::new(16, 158), Point::new(11, 160)),
    (Point::new(15, 156), Point::new(10, 158)),
    (Point::new(14, 154), Point::new(10, 156)),
    (Point::new(14, 152), Point::new(9, 154)),
    (Point::new(13, 151), Point::new(8, 152)),
    (Point::new(13, 149), Point::new(8, 150)),
    (Point::new(12, 147), Point::new(7, 148)),
    (Point::new(12, 145), Point::new(7, 146)),
    (Point::new(11, 143), Point::new(7, 144)),
    (Point::new(11, 141), Point::new(6, 142)),
    (Point::new(11, 139), Point::new(6, 140)),
    (Point::new(10, 137), Point::new(5, 138)),
    (Point::new(10, 135), Point::new(5, 136)),
    (Point::new(10, 134), Point::new(5, 134)),
    (Point::new(10, 132), Point::new(5, 132)),
    (Point::new(9, 130), Point::new(4, 130)),
    (Point::new(9, 128), Point::new(4, 128)),
    (Point::new(9, 126), Point::new(4, 126)),
    (Point::new(9, 124), Point::new(4, 124)),
    (Point::new(9, 122), Point::new(4, 122)),
    (Point::new(9, 120), Point::new(4, 120)),
    (Point::new(9, 118), Point::new(4, 118)),
    (Point::new(9, 116), Point::new(4, 116)),
    (Point::new(9, 114), Point::new(4, 114)),
    (Point::new(9, 112), Point::new(4, 112)),
    (Point::new(9, 110), Point::new(4, 110)),
    (Point::new(10, 108), Point::new(5, 108)),
    (Point::new(10, 106), Point::new(5, 106)),
    (Point::new(10, 105), Point::new(5, 104)),
    (Point::new(10, 103), Point::new(5, 102)),
    (Point::new(11, 101), Point::new(6, 100)),
    (Point::new(11, 99), Point::new(6, 98)),
    (Point::new(11, 97), Point::new(7, 96)),
    (Point::new(12, 95), Point::new(7, 94)),
    (Point::new(12, 93), Point::new(7, 92)),
    (Point::new(13, 91), Point::new(8, 90)),
    (Point::new(13, 89), Point::new(8, 88)),
    (Point::new(14, 88), Point::new(9, 86)),
    (Point::new(14, 86), Point::new(10, 84)),
    (Point::new(15, 84), Point::new(10, 82)),
    (Point::new(16, 82), Point::new(11, 80)),
    (Point::new(16, 80), Point::new(12, 78)),
    (Point::new(17, 78), Point::new(12, 77)),
    (Point::new(18, 77), Point::new(13, 75)),
    (Point::new(19, 75), Point::new(14, 73)),
    (Point::new(19, 73), Point::new(15, 71)),
    (Point::new(20, 71), Point::new(16, 69)),
    (Point::new(21, 70), Point::new(17, 67)),
    (Point::new(22, 68), Point::new(18, 66)),
    (Point::new(23, 66), Point::new(19, 64)),
    (Point::new(24, 64), Point::new(20, 62)),
    (Point::new(25, 63), Point::new(21, 60)),
    (Point::new(26, 61), Point::new(22, 59)),
    (Point::new(27, 60), Point::new(23, 57)),
    (Point::new(28, 58), Point::new(24, 55)),
    (Point::new(29, 56), Point::new(25, 53)),
    (Point::new(30, 55), Point::new(26, 52)),
    (Point::new(31, 53), Point::new(27, 50)),
    (Point::new(33, 52), Point::new(29, 49)),
    (Point::new(34, 50), Point::new(30, 47)),
    (Point::new(35, 49), Point::new(31, 45)),
    (Point::new(36, 47), Point::new(32, 44)),
    (Point::new(38, 46), Point::new(34, 42)),
    (Point::new(39, 44), Point::new(35, 41)),
    (Point::new(40, 43), Point::new(37, 39)),
];

/// Precomputed tick coordinates for RAM meter arc (-45° to 44°, 90 ticks)
const RAM_ARC_TICKS: [(Point, Point); 90] = [
    (Point::new(198, 42), Point::new(202, 38)),
    (Point::new(200, 43), Point::new(203, 39)),
    (Point::new(201, 44), Point::new(205, 41)),
    (Point::new(202, 46), Point::new(206, 42)),
    (Point::new(204, 47), Point::new(208, 44)),
    (Point::new(205, 49), Point::new(209, 45)),
    (Point::new(206, 50), Point::new(210, 47)),
    (Point::new(207, 52), Point::new(211, 49)),
    (Point::new(209, 53), Point::new(213, 50)),
    (Point::new(210, 55), Point::new(214, 52)),
    (Point::new(211, 56), Point::new(215, 53)),
    (Point::new(212, 58), Point::new(216, 55)),
    (Point::new(213, 60), Point::new(217, 57)),
    (Point::new(214, 61), Point::new(218, 59)),
    (Point::new(215, 63), Point::new(219, 60)),
    (Point::new(216, 64), Point::new(220, 62)),
    (Point::new(217, 66), Point::new(221, 64)),
    (Point::new(218, 68), Point::new(222, 66)),
    (Point::new(219, 70), Point::new(223, 67)),
    (Point::new(220, 71), Point::new(224, 69)),
    (Point::new(221, 73), Point::new(225, 71)),
    (Point::new(221, 75), Point::new(226, 73)),
    (Point::new(222, 77), Point::new(227, 75)),
    (Point::new(223, 78), Point::new(228, 77)),
    (Point::new(224, 80), Point::new(228, 78)),
    (Point::new(224, 82), Point::new(229, 80)),
    (Point::new(225, 84), Point::new(230, 82)),
    (Point::new(226, 86), Point::new(230, 84)),
    (Point::new(226, 88), Point::new(231, 86)),
    (Point::new(227, 89), Point::new(232, 88)),
    (Point::new(227, 91), Point::new(232, 90)),
    (Point::new(228, 93), Point::new(233, 92)),
    (Point::new(228, 95), Point::new(233, 94)),
    (Point::new(229, 97), Point::new(233, 96)),
    (Point::new(229, 99), Point::new(234, 98)),
    (Point::new(229, 101), Point::new(234, 100)),
    (Point::new(230, 103), Point::new(235, 102)),
    (Point::new(230, 105), Point::new(235, 104)),
    (Point::new(230, 106), Point::new(235, 106)),
    (Point::new(230, 108), Point::new(235, 108)),
    (Point::new(231, 110), Point::new(236, 110)),
    (Point::new(231, 112), Point::new(236, 112)),
    (Point::new(231, 114), Point::new(236, 114)),
    (Point::new(231, 116), Point::new(236, 116)),
    (Point::new(231, 118), Point::new(236, 118)),
    (Point::new(231, 120), Point::new(236, 120)),
    (Point::new(231, 122), Point::new(236, 122)),
    (Point::new(231, 124), Point::new(236, 124)),
    (Point::new(231, 126), Point::new(236, 126)),
    (Point::new(231, 128), Point::new(236, 128)),
    (Point::new(231, 130), Point::new(236, 130)),
    (Point::new(230, 132), Point::new(235, 132)),
    (Point::new(230, 134), Point::new(235, 134)),
    (Point::new(230, 135), Point::new(235, 136)),
    (Point::new(230, 137), Point::new(235, 138)),
    (Point::new(229, 139), Point::new(234, 140)),
    (Point::new(229, 141), Point::new(234, 142)),
    (Point::new(229, 143), Point::new(233, 144)),
    (Point::new(228, 145), Point::new(233, 146)),
    (Point::new(228, 147), Point::new(233, 148)),
    (Point::new(227, 149), Point::new(232, 150)),
    (Point::new(227, 151), Point::new(232, 152)),
    (Point::new(226, 152), Point::new(231, 154)),
    (Point::new(226, 154), Point::new(230, 156)),
    (Point::new(225, 156), Point::new(230, 158)),
    (Point::new(224, 158), Point::new(229, 160)),
    (Point::new(224, 160), Point::new(228, 162)),
    (Point::new(223, 162), Point::new(228, 163)),
    (Point::new(222, 163), Point::new(227, 165)),
    (Point::new(221, 165), Point::new(226, 167)),
    (Point::new(221, 167), Point::new(225, 169)),
    (Point::new(220, 169), Point::new(224, 171)),
    (Point::new(219, 170), Point::new(223, 173)),
    (Point::new(218, 172), Point::new(222, 174)),
    (Point::new(217, 174), Point::new(221, 176)),
    (Point::new(216, 176), Point::new(220, 178)),
    (Point::new(215, 177), Point::new(219, 180)),
    (Point::new(214, 179), Point::new(218, 181)),
    (Point::new(213, 180), Point::new(217, 183)),
    (Point::new(212, 182), Point::new(216, 185)),
    (Point::new(211, 184), Point::new(215, 187)),
    (Point::new(210, 185), Point::new(214, 188)),
    (Point::new(209, 187), Point::new(213, 190)),
    (Point::new(207, 188), Point::new(211, 191)),
    (Point::new(206, 190), Point::new(210, 193)),
    (Point::new(205, 191), Point::new(209, 195)),
    (Point::new(204, 193), Point::new(208, 196)),
    (Point::new(202, 194), Point::new(206, 198)),
    (Point::new(201, 196), Point::new(205, 199)),
    (Point::new(200, 197), Point::new(203, 201)),
];

/// Render CPU and RAM load meter arcs along circular bezel without runtime trigonometric computation (Zero-FPU LUT)
fn render_meter_arcs<D: DrawTarget<Color = Rgb565>>(target: &mut D, cpu_pct: u8, ram_pct: u8) {
    let cpu_fill_count = (((cpu_pct.min(100) as u32) * 90) / 100) as usize;
    let cpu_color = if cpu_pct > 80 { COLOR_RED } else { COLOR_CYAN };

    for (i, &(p1, p2)) in CPU_ARC_TICKS.iter().enumerate() {
        let c = if i < cpu_fill_count {
            cpu_color
        } else {
            COLOR_DARK_GRAY
        };
        Line::new(p1, p2)
            .into_styled(PrimitiveStyle::with_stroke(c, 1))
            .draw(target)
            .ok();
    }

    let ram_fill_count = (((ram_pct.min(100) as u32) * 90) / 100) as usize;
    let ram_color = if ram_pct > 85 {
        COLOR_AMBER
    } else {
        COLOR_GREEN
    };

    for (i, &(p1, p2)) in RAM_ARC_TICKS.iter().enumerate() {
        let c = if i < ram_fill_count {
            ram_color
        } else {
            COLOR_DARK_GRAY
        };
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
