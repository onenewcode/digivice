//! Mode D: Cyber Cyclops Pet UI View (赛博独眼灵动桌宠)
//!
//! Highly responsive, zero-FPU parametric single-eye digital pet.
//! Features gravity-tracked iris gaze, Pixar-style dual catchlights,
//! morphing pupil shapes, and squash-and-stretch eyelid animation.

use super::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_BG, COLOR_CAPSULE_BORDER, COLOR_CYAN, COLOR_DARK_GRAY,
    COLOR_GRAY, COLOR_GREEN, COLOR_RED, COLOR_WHITE, PetMood, PupilShape, UiState,
};
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{
    Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle, Triangle,
};
use embedded_graphics::text::Text;
use heapless::String;

// Domain Geometry Constants for Cyber Cyclops Eye (eliminates magic numbers)
pub const IRIS_HALO_RADIUS_PX: i32 = 56;
pub const IRIS_BODY_RADIUS_PX: i32 = 52;
pub const IRIS_INNER_RING_RADIUS_PX: i32 = 38;
pub const PUPIL_RADIUS_PX: i32 = 24;
pub const CATCHLIGHT_MAIN_RADIUS_PX: u32 = 6;
pub const CATCHLIGHT_SUB_RADIUS_PX: u32 = 3;

/// Precomputed 24 orbital tick marks on the 240x240 circular bezel (Zero-FPU LUT).
/// Radius outer: 114px, radius inner: 107px, 15 degrees per tick.
const ORBITAL_TICKS: [(Point, Point); 24] = [
    (Point::new(120, 6), Point::new(120, 13)),
    (Point::new(150, 10), Point::new(148, 17)),
    (Point::new(177, 21), Point::new(174, 27)),
    (Point::new(201, 39), Point::new(196, 44)),
    (Point::new(219, 63), Point::new(213, 66)),
    (Point::new(230, 90), Point::new(223, 92)),
    (Point::new(234, 120), Point::new(227, 120)),
    (Point::new(230, 150), Point::new(223, 148)),
    (Point::new(219, 177), Point::new(213, 174)),
    (Point::new(201, 201), Point::new(196, 196)),
    (Point::new(177, 219), Point::new(174, 213)),
    (Point::new(150, 230), Point::new(148, 223)),
    (Point::new(120, 234), Point::new(120, 227)),
    (Point::new(90, 230), Point::new(92, 223)),
    (Point::new(63, 219), Point::new(67, 213)),
    (Point::new(39, 201), Point::new(44, 196)),
    (Point::new(21, 177), Point::new(27, 174)),
    (Point::new(10, 150), Point::new(17, 148)),
    (Point::new(6, 120), Point::new(13, 120)),
    (Point::new(10, 90), Point::new(17, 92)),
    (Point::new(21, 63), Point::new(27, 66)),
    (Point::new(39, 39), Point::new(44, 44)),
    (Point::new(63, 21), Point::new(66, 27)),
    (Point::new(90, 10), Point::new(92, 17)),
];

/// Render the Cyber Cyclops pet view.
pub fn render_pet<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    // 1. Outer Orbital HUD Ring
    render_orbital_ring(target, state);

    // 2. Iris, Pupil and Catchlights
    render_iris_and_pupil(target, state);

    // 3. Eyelids (Blink & Smile Clipping)
    render_eyelids(target, state);

    // 4. Subtle Top/Bottom Status HUD
    render_pet_hud(target, state);
}

/// 1. Outer Orbital HUD Ring: rotating tick pulse reflecting mood & telemetry.
fn render_orbital_ring<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let pet = &state.pet;
    let tick_color = match pet.mood {
        PetMood::Happy => COLOR_GREEN,
        PetMood::Focused => COLOR_CYAN,
        PetMood::Dizzy => COLOR_AMBER,
        PetMood::Shocked => COLOR_RED,
        PetMood::Sleepy | PetMood::Sleeping => COLOR_DARK_GRAY,
        PetMood::Normal => {
            if state.telemetry.cpu_pct > 75 {
                COLOR_AMBER
            } else {
                COLOR_CYAN
            }
        }
    };

    let active_count = match pet.mood {
        PetMood::Sleeping => 4,
        PetMood::Happy | PetMood::Focused => 24,
        _ => 16,
    };

    // Spin offset based on animation tick
    let offset = ((pet.anim_tick / 4) % 24) as usize;

    for (i, &(p1, p2)) in ORBITAL_TICKS.iter().enumerate() {
        let is_active = ((i + offset) % 24) < active_count;
        let c = if is_active {
            tick_color
        } else {
            COLOR_DARK_GRAY
        };
        Line::new(p1, p2)
            .into_styled(PrimitiveStyle::with_stroke(c, 1))
            .draw(target)
            .ok();
    }
}

/// 2. Floating Iris & Morphing Pupil.
fn render_iris_and_pupil<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let pet = &state.pet;

    // Center offset clamped to safe radius (center: 120, 120)
    let gx = (pet.gaze_x as i32).clamp(-18, 18);
    let gy = (pet.gaze_y as i32).clamp(-18, 18);
    let center = Point::new(CENTER_X + gx, CENTER_Y + gy);

    // Skip iris rendering when fully closed (sleeping / mid-blink)
    if pet.blink_pct == 0 {
        return;
    }

    // Determine Iris primary color
    let (iris_fill, iris_border) = match pet.mood {
        PetMood::Happy => (COLOR_GREEN, COLOR_WHITE),
        PetMood::Focused => (COLOR_CYAN, COLOR_WHITE),
        PetMood::Dizzy => (COLOR_AMBER, COLOR_RED),
        PetMood::Shocked => (COLOR_RED, COLOR_WHITE),
        PetMood::Sleepy | PetMood::Sleeping => (COLOR_DARK_GRAY, COLOR_GRAY),
        PetMood::Normal => (COLOR_CYAN, COLOR_CAPSULE_BORDER),
    };

    // A. Outer Iris Halo / Eye Socket Ring
    Circle::new(
        center - Point::new(IRIS_HALO_RADIUS_PX, IRIS_HALO_RADIUS_PX),
        (IRIS_HALO_RADIUS_PX * 2) as u32,
    )
    .into_styled(PrimitiveStyle::with_stroke(COLOR_DARK_GRAY, 1))
    .draw(target)
    .ok();

    // B. Solid Iris Body
    let iris_style = PrimitiveStyleBuilder::new()
        .fill_color(iris_fill)
        .stroke_color(iris_border)
        .stroke_width(2)
        .build();
    Circle::new(
        center - Point::new(IRIS_BODY_RADIUS_PX, IRIS_BODY_RADIUS_PX),
        (IRIS_BODY_RADIUS_PX * 2) as u32,
    )
    .into_styled(iris_style)
    .draw(target)
    .ok();

    // C. Concentric Sci-Fi Inner Ring
    Circle::new(
        center - Point::new(IRIS_INNER_RING_RADIUS_PX, IRIS_INNER_RING_RADIUS_PX),
        (IRIS_INNER_RING_RADIUS_PX * 2) as u32,
    )
    .into_styled(PrimitiveStyle::with_stroke(COLOR_BG, 1))
    .draw(target)
    .ok();

    // D. Pupil Rendering based on PupilShape
    render_pupil(target, center, pet.pupil_shape, pet.anim_tick);
}

/// Draw the morphing pupil with catchlights.
fn render_pupil<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    center: Point,
    shape: PupilShape,
    tick: u16,
) {
    match shape {
        PupilShape::Round => {
            // Standard Deep Black Pupil
            Circle::new(
                center - Point::new(PUPIL_RADIUS_PX, PUPIL_RADIUS_PX),
                (PUPIL_RADIUS_PX * 2) as u32,
            )
            .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
            .draw(target)
            .ok();

            // Pixar-Style Dual White Catchlights (Glints)
            // Primary catchlight: upper-left
            Circle::new(center + Point::new(-14, -14), CATCHLIGHT_MAIN_RADIUS_PX * 2)
                .into_styled(PrimitiveStyle::with_fill(COLOR_WHITE))
                .draw(target)
                .ok();

            // Secondary subtle catchlight: lower-right
            Circle::new(center + Point::new(8, 8), CATCHLIGHT_SUB_RADIUS_PX * 2)
                .into_styled(PrimitiveStyle::with_fill(COLOR_WHITE))
                .draw(target)
                .ok();
        }
        PupilShape::CatSlit => {
            // Narrow vertical cat slit for extreme focus
            let slit_rect = Rectangle::new(center - Point::new(6, 26), Size::new(12, 52));
            RoundedRectangle::with_equal_corners(slit_rect, Size::new(6, 6))
                .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
                .draw(target)
                .ok();

            // Catchlight dot
            Circle::new(center + Point::new(-2, -10), 4)
                .into_styled(PrimitiveStyle::with_fill(COLOR_WHITE))
                .draw(target)
                .ok();
        }
        PupilShape::Heart => {
            // Deep black backdrop
            Circle::new(center - Point::new(24, 24), 48)
                .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
                .draw(target)
                .ok();

            // Geometric Heart Shape (two circular lobes + downward triangle)
            Circle::new(center + Point::new(-11, -9), 12)
                .into_styled(PrimitiveStyle::with_fill(COLOR_RED))
                .draw(target)
                .ok();
            Circle::new(center + Point::new(-1, -9), 12)
                .into_styled(PrimitiveStyle::with_fill(COLOR_RED))
                .draw(target)
                .ok();
            Triangle::new(
                center + Point::new(-11, -3),
                center + Point::new(11, -3),
                center + Point::new(0, 11),
            )
            .into_styled(PrimitiveStyle::with_fill(COLOR_RED))
            .draw(target)
            .ok();

            // Tiny catchlight on heart lobe
            Circle::new(center + Point::new(-7, -7), 3)
                .into_styled(PrimitiveStyle::with_fill(COLOR_WHITE))
                .draw(target)
                .ok();
        }
        PupilShape::Star => {
            Circle::new(center - Point::new(24, 24), 48)
                .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
                .draw(target)
                .ok();

            // 4-point star in amber/gold
            Triangle::new(
                center + Point::new(-12, 0),
                center + Point::new(12, 0),
                center + Point::new(0, -14),
            )
            .into_styled(PrimitiveStyle::with_fill(COLOR_AMBER))
            .draw(target)
            .ok();
            Triangle::new(
                center + Point::new(-12, 0),
                center + Point::new(12, 0),
                center + Point::new(0, 14),
            )
            .into_styled(PrimitiveStyle::with_fill(COLOR_AMBER))
            .draw(target)
            .ok();
            Circle::new(center - Point::new(4, 4), 8)
                .into_styled(PrimitiveStyle::with_fill(COLOR_WHITE))
                .draw(target)
                .ok();
        }
        PupilShape::Dizzy => {
            // Alternating dizzy concentric spirals
            let r_offset = (tick % 4) as i32;
            Circle::new(center - Point::new(24, 24), 48)
                .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
                .draw(target)
                .ok();
            Circle::new(
                center - Point::new(20 - r_offset, 20 - r_offset),
                ((20 - r_offset) * 2) as u32,
            )
            .into_styled(PrimitiveStyle::with_stroke(COLOR_AMBER, 2))
            .draw(target)
            .ok();
            Circle::new(
                center - Point::new(12 - r_offset, 12 - r_offset),
                ((12 - r_offset) * 2) as u32,
            )
            .into_styled(PrimitiveStyle::with_stroke(COLOR_WHITE, 2))
            .draw(target)
            .ok();
        }
        PupilShape::Lightning => {
            Circle::new(center - Point::new(24, 24), 48)
                .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
                .draw(target)
                .ok();

            // Sci-fi lightning bolt in amber
            Line::new(center + Point::new(3, -16), center + Point::new(-6, 2))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_AMBER, 3))
                .draw(target)
                .ok();
            Line::new(center + Point::new(-6, 2), center + Point::new(5, 2))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_AMBER, 3))
                .draw(target)
                .ok();
            Line::new(center + Point::new(5, 2), center + Point::new(-4, 18))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_AMBER, 3))
                .draw(target)
                .ok();
        }
        PupilShape::Cross => {
            Circle::new(center - Point::new(24, 24), 48)
                .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
                .draw(target)
                .ok();

            // Red cross error / knockout
            Line::new(center + Point::new(-12, -12), center + Point::new(12, 12))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_RED, 3))
                .draw(target)
                .ok();
            Line::new(center + Point::new(-12, 12), center + Point::new(12, -12))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_RED, 3))
                .draw(target)
                .ok();
        }
    }
}

/// 3. Eyelids: Smooth zero-FPU integer clipping for blinking and smiling.
fn render_eyelids<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let pet = &state.pet;

    // Normal eye vertical span: Y from ~60 to ~180 (height = 120)
    let blink_pct = pet.blink_pct.min(105);

    if blink_pct < 100 {
        // Upper eyelid drops down, lower eyelid rises up
        let drop = ((100 - blink_pct as i32) * 60) / 100;

        let top_lid_y = 60 + drop;
        let bot_lid_y = 180 - drop;

        // Top eyelid block
        if top_lid_y > 0 {
            Rectangle::new(Point::new(0, 0), Size::new(240, top_lid_y as u32))
                .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
                .draw(target)
                .ok();
        }

        // Bottom eyelid block
        if bot_lid_y < 240 {
            Rectangle::new(
                Point::new(0, bot_lid_y),
                Size::new(240, (240 - bot_lid_y) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
            .draw(target)
            .ok();
        }

        // When almost fully closed, draw a calm curved sleep line
        if blink_pct <= 10 {
            Line::new(Point::new(76, 120), Point::new(164, 120))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_WHITE, 2))
                .draw(target)
                .ok();
            Line::new(Point::new(68, 116), Point::new(76, 120))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_GRAY, 1))
                .draw(target)
                .ok();
            Line::new(Point::new(164, 120), Point::new(172, 116))
                .into_styled(PrimitiveStyle::with_stroke(COLOR_GRAY, 1))
                .draw(target)
                .ok();
        }
    }

    // Smiling Crescent: Lower eyelid lifts upward into a cheerful arch
    if pet.happy_lift_pct > 0 && pet.blink_pct > 20 {
        let lift = ((pet.happy_lift_pct as i32).min(100) * 44) / 100;
        // Draw an inverted arc mask from bottom in COLOR_BG
        // Circle centered below screen (120, 240 - lift + 60) with radius 100
        let mask_center = Point::new(CENTER_X, 240 - lift + 50);
        Circle::new(mask_center - Point::new(100, 100), 200)
            .into_styled(PrimitiveStyle::with_fill(COLOR_BG))
            .draw(target)
            .ok();

        // Cheerful upward smile accent curve along the edge
        Circle::new(mask_center - Point::new(100, 100), 200)
            .into_styled(PrimitiveStyle::with_stroke(COLOR_GREEN, 2))
            .draw(target)
            .ok();
    }
}

/// 4. Minimal Geek Status Header & Footer.
fn render_pet_hud<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let pet = &state.pet;

    // A. Top Mood Label (Y: 28)
    let mut header_str = String::<24>::new();
    match pet.mood {
        PetMood::Happy => {
            let _ = header_str.push_str("PET: HAPPY ^_^");
        }
        PetMood::Focused => {
            let _ = write!(header_str, "WORK: CPU {}%", state.telemetry.cpu_pct);
        }
        PetMood::Dizzy => {
            let _ = header_str.push_str("PET: DIZZY @_@");
        }
        PetMood::Sleeping => {
            let _ = header_str.push_str("PET: SLEEPING zZ");
        }
        PetMood::Sleepy => {
            let _ = header_str.push_str("PET: SLEEPY -_-");
        }
        PetMood::Shocked => {
            let _ = header_str.push_str("PET: O_O !");
        }
        PetMood::Normal => {
            let _ = write!(header_str, "PET: IDLE [{}%]", pet.energy);
        }
    }

    let header_color = match pet.mood {
        PetMood::Happy => COLOR_GREEN,
        PetMood::Focused => COLOR_CYAN,
        PetMood::Dizzy => COLOR_AMBER,
        PetMood::Shocked => COLOR_RED,
        _ => COLOR_GRAY,
    };

    Text::new(
        &header_str,
        Point::new(74, 32),
        MonoTextStyle::new(&FONT_6X10, header_color),
    )
    .draw(target)
    .ok();

    // B. Bottom Dock Status: Voltage & Affinity (Y: 216)
    let mut dock_str = String::<32>::new();
    let volts = state.v_rail_mv / 1000;
    let centivolts = (state.v_rail_mv % 1000) / 10;
    let _ = write!(
        dock_str,
        "{}.{:02}V  AFFINITY: {}",
        volts, centivolts, pet.affinity
    );

    Text::new(
        &dock_str,
        Point::new(56, 216),
        MonoTextStyle::new(&FONT_6X10, COLOR_GRAY),
    )
    .draw(target)
    .ok();
}
