//! Digital Monster Card Status Screen UI View
//!
//! Provides a canonical Digivice pet status screen displaying species tier,
//! pixel portrait, hunger/power meters, energy, battery level, and BLE presence.

use super::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_CYAN, COLOR_DARK_GRAY, COLOR_GREEN, COLOR_WHITE, RADIUS,
    UiState,
};
use crate::board::display::BandTarget;
use crate::pet::SpriteFlip;
use crate::pet::assets::{
    AGUMON_BODY_NEUTRAL, GREYMON_BODY_NEUTRAL, KOROMON_BODY_NEUTRAL, METAL_GREYMON_BODY_NEUTRAL,
    OMEGAMON_BODY_NEUTRAL, WAR_GREYMON_BODY_NEUTRAL,
};
use crate::pet::species::PetSpecies;
use crate::radio::{PresenceState, RadioStatus};
use core::fmt::Write;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;

const COLOR_BORDER: Rgb565 = Rgb565::new(4, 20, 16);

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
fn render_status_header(target: &mut BandTarget, state: &UiState) {
    // 1. Outer cyber cyan perimeter ring (R=118, Center 120,120)
    Circle::new(
        Point::new(CENTER_X - RADIUS, CENTER_Y - RADIUS),
        (RADIUS * 2) as u32,
    )
    .into_styled(PrimitiveStyle::with_stroke(COLOR_CYAN, 1))
    .draw(target)
    .ok();

    // 2. Top Header: Species Name & Evolution Tier Badge (Y: 26..38)
    let mut header_buf = String::<36>::new();
    let _ = write!(
        header_buf,
        "{} [{}]",
        state.pet.species.name(),
        state.pet.species.tier_name()
    );
    draw_label(target, &header_buf, Point::new(38, 36), COLOR_CYAN);

    // Separator line under header (Y: 42)
    Line::new(Point::new(36, 42), Point::new(204, 42))
        .into_styled(PrimitiveStyle::with_stroke(COLOR_BORDER, 1))
        .draw(target)
        .ok();
}

#[inline(never)]
fn render_status_portrait(target: &mut BandTarget, state: &UiState) {
    let band_y = target.y_offset;

    // Central 48x48 Digital Monster Pixel Portrait with cyber frame (X: 96, Y: 48)
    Rectangle::new(Point::new(94, 47), Size::new(52, 52))
        .into_styled(PrimitiveStyle::with_stroke(COLOR_BORDER, 1))
        .draw(target)
        .ok();

    let sprite = match state.pet.species {
        PetSpecies::Koromon => &KOROMON_BODY_NEUTRAL,
        PetSpecies::Agumon => &AGUMON_BODY_NEUTRAL,
        PetSpecies::Greymon => &GREYMON_BODY_NEUTRAL,
        PetSpecies::MetalGreymon => &METAL_GREYMON_BODY_NEUTRAL,
        PetSpecies::WarGreymon => &WAR_GREYMON_BODY_NEUTRAL,
        PetSpecies::Omegamon => &OMEGAMON_BODY_NEUTRAL,
    };
    sprite.blit_to_band_scaled(
        &mut target.buffer[..],
        band_y,
        96,
        49,
        1,
        SpriteFlip::Normal,
    );
}

#[inline(never)]
fn render_status_telemetry(target: &mut BandTarget, state: &UiState) {
    let mut line_buf = String::<36>::new();

    // Line 1: Age & Weight (Y: 108)
    let _ = write!(
        line_buf,
        "AGE:{}d       WT:{}g",
        state.pet.age_days, state.pet.weight_g
    );
    draw_label(target, &line_buf, Point::new(36, 108), COLOR_CYAN);

    // Line 2: Hunger & Strength Meters (Y: 122)
    line_buf.clear();
    let _ = write!(
        line_buf,
        "HUNGER:{}/4   PWR:{}/4",
        state.pet.hunger, state.pet.strength
    );
    draw_label(target, &line_buf, Point::new(36, 122), COLOR_WHITE);

    // Line 3: Training Sessions & Care Mistakes (Y: 136)
    line_buf.clear();
    let _ = write!(
        line_buf,
        "TRAIN:{}     ERR:{}",
        state.pet.training_count, state.pet.care_mistakes
    );
    draw_label(target, &line_buf, Point::new(36, 136), COLOR_AMBER);

    // Line 4: Energy & Affinity (Y: 150)
    line_buf.clear();
    let _ = write!(
        line_buf,
        "ENERGY:{}%   AFF:{}",
        state.pet.energy, state.pet.affinity
    );
    draw_label(target, &line_buf, Point::new(36, 150), COLOR_WHITE);

    // Line 5: Battery Rail & Presence Link (Y: 164)
    line_buf.clear();
    let pres_str = match state.radio_status {
        RadioStatus::Active => match state.presence {
            PresenceState::Near => "NEAR",
            PresenceState::Away => "AWAY",
        },
        RadioStatus::Disabled => "OFFLINE",
        RadioStatus::ControllerUnavailable => "NO_CTRL",
        RadioStatus::GattFailed => "GATT_ERR",
        RadioStatus::RunnerExited => "RUN_ERR",
    };
    let _ = write!(line_buf, "BAT:{}mV BLE:{}", state.v_rail_mv, pres_str);
    draw_label(target, &line_buf, Point::new(36, 164), COLOR_GREEN);

    line_buf.clear();
    let _ = write!(
        line_buf,
        "SAVE:{} #{}",
        state.save_status.label(),
        state.save_status.committed_seq()
    );
    draw_label(target, &line_buf, Point::new(36, 178), COLOR_AMBER);

    // Navigation Footer Hint (Y: 196)
    draw_label(
        target,
        "[BOOT: PET VIEW]",
        Point::new(72, 196),
        COLOR_DARK_GRAY,
    );
}

/// Render the canonical Digivice Digital Monster Card Status screen.
pub fn render_status(target: &mut BandTarget, state: &UiState) {
    render_status_header(target, state);
    render_status_portrait(target, state);
    render_status_telemetry(target, state);
}
