//! Full-Color Koromon Mascot & Circular Audio Visualizer Renderer.
//!
//! Replaces legacy vector math with high-performance 4bpp sprite blitting,
//! dynamic Squash & Stretch positioning, modular anatomy assembly, and outer neon EQ ring.

use super::UiState;
use crate::board::display::{BAND_HEIGHT, BandTarget};
use crate::pet::assets::{
    AGUMON_ATTACK_FIRE, AGUMON_BODY_ACTION, AGUMON_BODY_NEUTRAL, DIGIMON_16_PALETTE, FX_BANDAGE,
    FX_HEART, FX_NOTE, FX_SKULL, GREYMON_ATTACK_MEGA_FLAME, GREYMON_BODY_ACTION,
    GREYMON_BODY_NEUTRAL, HUD_CALL_ATTENTION, ITEM_POOP, ITEM_PROTEIN_PILL, ITEM_ROAST_MEAT,
    KOROMON_BODY_ACTION, KOROMON_BODY_NEUTRAL, METAL_GREYMON_ATTACK_MISSILES,
    METAL_GREYMON_BODY_ACTION, METAL_GREYMON_BODY_NEUTRAL, OMEGAMON_ATTACK_SUPREME_CANNON,
    OMEGAMON_BODY_ACTION, OMEGAMON_BODY_NEUTRAL, PROP_HEADBAND, PROP_HEADPHONES,
    WAR_GREYMON_ATTACK_GAIA_FORCE, WAR_GREYMON_BODY_ACTION, WAR_GREYMON_BODY_NEUTRAL, sin_cos_q12,
};
use crate::pet::species::{AttentionState, FoodType, MusicDanceStyle, PetAction, PetSpecies};
use crate::pet::{PetMood, PupilShape, SpriteFlip};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};

const COLOR_PLATFORM: Rgb565 = Rgb565::new(3, 10, 8); // Cyber circular platform
const COLOR_PLATFORM_RIM: Rgb565 = Rgb565::new(7, 24, 18);
const COLOR_EQ_CYAN: Rgb565 = Rgb565::new(7, 43, 24); // Neon rhythm cyan
const COLOR_EQ_GOLD: Rgb565 = Rgb565::new(31, 49, 8);
const COLOR_HUNGER_HEART: Rgb565 = Rgb565::new(27, 4, 4); // Ruby red
const COLOR_ENERGY_BOLT: Rgb565 = Rgb565::new(31, 50, 0); // Golden yellow
const COLOR_EMPTY_SLOT: Rgb565 = Rgb565::new(6, 12, 10); // Dim grey

/// 16-point perimeter coordinates (radius ~114px) for the circular neon EQ visualizer.
const EQ_POINTS: [(i32, i32); 16] = [
    (232, 120),
    (223, 163),
    (199, 199),
    (163, 223),
    (120, 232),
    (77, 223),
    (41, 199),
    (17, 163),
    (8, 120),
    (17, 77),
    (41, 41),
    (77, 17),
    (120, 8),
    (163, 17),
    (199, 41),
    (223, 77),
];

pub use super::dirty::pet_dirty_y_bounds;

/// Render the full-color Koromon pet frame onto a single GC9A01 horizontal band target.
pub fn render_pet(target: &mut BandTarget, state: &UiState) {
    let pet = &state.pet;
    let band_y = target.y_offset;

    // 1. Draw Background & Tech Platform
    render_background_and_platform(target, band_y);

    // 2. Draw Circular Outer Audio EQ Visualizer if music is active
    if pet.music_beat_pulse > 0 || state.music.bpm > 0 {
        render_circular_audio_eq(target, band_y, pet.music_beat_pulse, state.music.energy);
    }

    // 3. Compute Pet's Dynamic Spatial Center & Floor Baseline (with inertial tilt, squash offset, and autonomous wander offset)
    let base_x: i32 = 120 + (pet.wander_x as i32) + (pet.gaze_x as i32 / 2);
    let floor_y: i32 = 164 + pet.squash.offset_y_px as i32 + (pet.gaze_y as i32 / 3);

    // 4. Assemble Modular Anatomy Layers directly into the band buffer
    match pet.species {
        PetSpecies::Koromon => {
            render_koromon_anatomy(&mut target.buffer[..], band_y, base_x, floor_y, pet);
        }
        PetSpecies::Agumon => {
            render_agumon_anatomy(&mut target.buffer[..], band_y, base_x, floor_y, pet);
        }
        PetSpecies::Greymon => {
            render_greymon_anatomy(&mut target.buffer[..], band_y, base_x, floor_y, pet);
        }
        PetSpecies::MetalGreymon => {
            render_metal_greymon_anatomy(&mut target.buffer[..], band_y, base_x, floor_y, pet);
        }
        PetSpecies::WarGreymon => {
            render_wargreymon_anatomy(&mut target.buffer[..], band_y, base_x, floor_y, pet);
        }
        PetSpecies::Omegamon => {
            render_omegamon_anatomy(&mut target.buffer[..], band_y, base_x, floor_y, pet);
        }
    }

    // 5. Render props and floating particles
    render_props_and_particles(&mut target.buffer[..], band_y, base_x, floor_y, pet);

    // 6. Draw Top Stats HUD (Hunger Hearts, Strength Bolts & Attention Icon) as topmost overlay
    render_stats_hud(
        target,
        band_y,
        pet.hunger,
        pet.strength,
        pet.attention,
        pet.anim_tick,
    );

    // 7. Draw Pendulum Shake Training Gauge if actively shaking
    if pet.training_shakes > 0 || matches!(pet.action, PetAction::ShakeChargeSprint { .. }) {
        render_pendulum_training_hud(target, band_y, pet.training_shakes);
    }
}

/// Render dark cyber background and circular platform floor.
fn render_background_and_platform(target: &mut BandTarget, band_y: i32) {
    let band_bottom = band_y + BAND_HEIGHT as i32;
    // Cyber pedestal platform under Koromon (Y: 160..178)
    if 178 < band_y || 160 >= band_bottom {
        return;
    }

    // 1. Cyber Pedestal Base (rounded platform spanning X: 52..188)
    embedded_graphics::primitives::RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(52, 162), Size::new(136, 14)),
        Size::new(7, 7),
    )
    .into_styled(PrimitiveStyle::with_fill(COLOR_PLATFORM))
    .draw(target)
    .ok();

    // 2. Neon cyan rim line on platform surface
    Line::new(Point::new(58, 164), Point::new(182, 164))
        .into_styled(PrimitiveStyle::with_stroke(COLOR_PLATFORM_RIM, 2))
        .draw(target)
        .ok();
}

/// Render outer circular neon audio visualizer ring reacting to BPM beat phase.
fn render_circular_audio_eq(target: &mut BandTarget, band_y: i32, beat_phase: u8, energy: u8) {
    let intensity = (energy as i32 * 8) / 100;
    let pulse_len = ((beat_phase as i32 * 6) / 255).min(6);
    let radius = (2 + (intensity + pulse_len) / 4) as u32;
    let radius_i32 = radius as i32;
    let band_bottom = band_y + BAND_HEIGHT as i32;

    for (angle_idx, &(cx, cy)) in EQ_POINTS.iter().enumerate() {
        // 1D AABB band culling: skip circles that do not intersect this band
        if cy + radius_i32 < band_y || cy - radius_i32 >= band_bottom {
            continue;
        }

        let color = if (angle_idx & 1) == 0 {
            COLOR_EQ_CYAN
        } else {
            COLOR_EQ_GOLD
        };

        Circle::new(Point::new(cx - radius_i32, cy - radius_i32), radius * 2)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)
            .ok();
    }
}

/// Blit Koromon's official Bandai 48x48 2-frame sprite and Accessories (2x Authentic Pixel Scale).
fn render_koromon_anatomy(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2; // High-Fidelity 2x scale (48x48 -> 96x96px, crisp 2x2 fine pixels)

    let flip = if pet.tilt_deg > 0 || (pet.tilt_deg == 0 && pet.gaze_x > 0) {
        SpriteFlip::Flipped
    } else {
        SpriteFlip::Normal
    };

    let is_walk_step =
        matches!(pet.action, PetAction::PivotWalk { .. }) && (pet.anim_tick / 8 % 2 == 1);
    let walk_bob_y = if is_walk_step { -2 } else { 0 };

    let is_action = is_walk_step
        || matches!(
            pet.action,
            PetAction::HappyHop
                | PetAction::VictoryHop
                | PetAction::AttackSpecial { .. }
                | PetAction::MusicDance { .. }
                | PetAction::SuspendedKick
                | PetAction::ShakeChargeSprint { .. }
        );

    let body_sprite = if is_action {
        &KOROMON_BODY_ACTION
    } else {
        &KOROMON_BODY_NEUTRAL
    };

    // Koromon base in DMC 48x48 box is at row 45 (90px height scaled by 2)
    let body_ox = base_x - 48;
    let body_oy = floor_y - 90 + walk_bob_y;
    let center_x = base_x;
    let center_y = body_oy + 48;

    if pet.tilt_deg != 0 {
        body_sprite.blit_to_band_rotated(
            buffer,
            band_y,
            (center_x, center_y),
            SCALE,
            flip,
            pet.tilt_deg,
        );
    } else {
        body_sprite.blit_to_band_scaled(buffer, band_y, body_ox, body_oy, SCALE, flip);
    }

    // Eyelid closure overlay when blinking or sleeping
    if (pet.blink_pct < 50 || pet.is_sleeping()) && pet.tilt_deg == 0 {
        let eye_y = body_oy + 44;
        let (left_x_start, right_x_start) = if flip == SpriteFlip::Normal {
            (base_x - 12, base_x + 12)
        } else {
            (base_x - 20, base_x + 4)
        };
        let eyelid_color = 0x2020; // Contour dark
        for dy in 0..2 {
            let py = eye_y + dy;
            if py >= band_y && py < band_y + BAND_HEIGHT as i32 {
                let row_offset = (py - band_y) as usize * 240;
                for dx in 0..8 {
                    let lx = left_x_start + dx;
                    let rx = right_x_start + dx;
                    if (0..240).contains(&lx) {
                        buffer[row_offset + lx as usize] = eyelid_color;
                    }
                    if (0..240).contains(&rx) {
                        buffer[row_offset + rx as usize] = eyelid_color;
                    }
                }
            }
        }
    }

    // Headband (if training or victory)
    if matches!(
        pet.action,
        PetAction::TrainingHop { .. }
            | PetAction::VictoryHop
            | PetAction::MusicDance {
                style: MusicDanceStyle::RhythmNod,
                ..
            }
    ) {
        if pet.tilt_deg != 0 {
            PROP_HEADBAND.blit_to_band_rotated(
                buffer,
                band_y,
                accessory_center((center_x, center_y), (-16, -6), pet.tilt_deg),
                SCALE,
                flip,
                pet.tilt_deg,
            );
        } else {
            PROP_HEADBAND.blit_to_band_scaled(
                buffer,
                band_y,
                base_x - 48,
                body_oy + 38,
                SCALE,
                flip,
            );
        }
    }

    // Headphones (if chill music dance)
    if matches!(
        pet.action,
        PetAction::MusicDance {
            style: MusicDanceStyle::ChillSway,
            ..
        }
    ) {
        if pet.tilt_deg != 0 {
            PROP_HEADPHONES.blit_to_band_rotated(
                buffer,
                band_y,
                accessory_center((center_x, center_y), (-16, -14), pet.tilt_deg),
                SCALE,
                flip,
                pet.tilt_deg,
            );
        } else {
            PROP_HEADPHONES.blit_to_band_scaled(
                buffer,
                band_y,
                base_x - 48,
                body_oy + 24,
                SCALE,
                flip,
            );
        }
    }
}

/// Rotate an accessory's established local center around the body pivot.
#[inline]
fn accessory_center(center: (i32, i32), local: (i32, i32), angle_deg: i16) -> (i32, i32) {
    let (sin_q12, cos_q12) = sin_cos_q12(angle_deg.rem_euclid(360));
    let x = (local.0 * cos_q12 - local.1 * sin_q12) >> 12;
    let y = (local.0 * sin_q12 + local.1 * cos_q12) >> 12;
    (center.0 + x, center.1 + y)
}

/// Blit Agumon's official Bandai 48x48 2-frame sprite and Pepper Breath fireball.
fn render_agumon_anatomy(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2;

    let flip = if pet.tilt_deg > 0 || (pet.tilt_deg == 0 && pet.gaze_x > 0) {
        SpriteFlip::Flipped
    } else {
        SpriteFlip::Normal
    };

    let is_walk_step =
        matches!(pet.action, PetAction::PivotWalk { .. }) && (pet.anim_tick / 8 % 2 == 1);
    let walk_bob_y = if is_walk_step { -2 } else { 0 };

    let is_action = is_walk_step
        || matches!(
            pet.action,
            PetAction::HappyHop
                | PetAction::VictoryHop
                | PetAction::AttackSpecial { .. }
                | PetAction::MusicDance { .. }
                | PetAction::SuspendedKick
                | PetAction::ShakeChargeSprint { .. }
        );

    let body_sprite = if is_action {
        &AGUMON_BODY_ACTION
    } else {
        &AGUMON_BODY_NEUTRAL
    };

    let body_ox = base_x - 48;
    let body_oy = floor_y - 96 + walk_bob_y;
    let center_x = base_x;
    let center_y = body_oy + 48;

    if pet.tilt_deg != 0 {
        body_sprite.blit_to_band_rotated(
            buffer,
            band_y,
            (center_x, center_y),
            SCALE,
            flip,
            pet.tilt_deg,
        );
    } else {
        body_sprite.blit_to_band_scaled(buffer, band_y, body_ox, body_oy, SCALE, flip);
    }

    // Pepper Breath Fireball Effect (Special attack, training, or victory)
    if matches!(
        pet.action,
        PetAction::AttackSpecial { .. } | PetAction::VictoryHop
    ) {
        let fx_ox = if flip.is_flipped() {
            base_x + 24
        } else {
            base_x - 56
        };
        AGUMON_ATTACK_FIRE.blit_to_band_scaled(buffer, band_y, fx_ox, body_oy + 30, SCALE, flip);
    }
}

/// Blit Greymon's official Bandai 48x48 2-frame sprite and Mega Flame.
fn render_greymon_anatomy(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2;

    let flip = if pet.tilt_deg > 0 || (pet.tilt_deg == 0 && pet.gaze_x > 0) {
        SpriteFlip::Flipped
    } else {
        SpriteFlip::Normal
    };

    let is_walk_step =
        matches!(pet.action, PetAction::PivotWalk { .. }) && (pet.anim_tick / 8 % 2 == 1);
    let walk_bob_y = if is_walk_step { -2 } else { 0 };

    let is_action = is_walk_step
        || matches!(
            pet.action,
            PetAction::HappyHop
                | PetAction::VictoryHop
                | PetAction::AttackSpecial { .. }
                | PetAction::MusicDance { .. }
                | PetAction::SuspendedKick
                | PetAction::ShakeChargeSprint { .. }
        );

    let body_sprite = if is_action {
        &GREYMON_BODY_ACTION
    } else {
        &GREYMON_BODY_NEUTRAL
    };

    let body_ox = base_x - 48;
    let body_oy = floor_y - 96 + walk_bob_y;
    let center_x = base_x;
    let center_y = body_oy + 48;

    if pet.tilt_deg != 0 {
        body_sprite.blit_to_band_rotated(
            buffer,
            band_y,
            (center_x, center_y),
            SCALE,
            flip,
            pet.tilt_deg,
        );
    } else {
        body_sprite.blit_to_band_scaled(buffer, band_y, body_ox, body_oy, SCALE, flip);
    }

    // Mega Flame Colossal Firestorm (Special attack, training, or victory)
    if matches!(
        pet.action,
        PetAction::AttackSpecial { .. } | PetAction::VictoryHop
    ) {
        let fx_ox = if flip.is_flipped() {
            base_x + 24
        } else {
            base_x - 60
        };
        GREYMON_ATTACK_MEGA_FLAME.blit_to_band_scaled(
            buffer,
            band_y,
            fx_ox,
            body_oy + 24,
            SCALE,
            flip,
        );
    }
}

/// Blit MetalGreymon's official Bandai 48x48 2-frame sprite and Giga Destroyer missiles.
fn render_metal_greymon_anatomy(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2;

    let flip = if pet.tilt_deg > 0 || (pet.tilt_deg == 0 && pet.gaze_x > 0) {
        SpriteFlip::Flipped
    } else {
        SpriteFlip::Normal
    };

    let is_walk_step =
        matches!(pet.action, PetAction::PivotWalk { .. }) && (pet.anim_tick / 8 % 2 == 1);
    let walk_bob_y = if is_walk_step { -2 } else { 0 };

    let is_action = is_walk_step
        || matches!(
            pet.action,
            PetAction::HappyHop
                | PetAction::VictoryHop
                | PetAction::AttackSpecial { .. }
                | PetAction::MusicDance { .. }
                | PetAction::SuspendedKick
                | PetAction::ShakeChargeSprint { .. }
        );

    let body_sprite = if is_action {
        &METAL_GREYMON_BODY_ACTION
    } else {
        &METAL_GREYMON_BODY_NEUTRAL
    };

    let body_ox = base_x - 48;
    let body_oy = floor_y - 96 + walk_bob_y;
    let center_x = base_x;
    let center_y = body_oy + 48;

    if pet.tilt_deg != 0 {
        body_sprite.blit_to_band_rotated(
            buffer,
            band_y,
            (center_x, center_y),
            SCALE,
            flip,
            pet.tilt_deg,
        );
    } else {
        body_sprite.blit_to_band_scaled(buffer, band_y, body_ox, body_oy, SCALE, flip);
    }

    // Giga Destroyer Twin Cruise Missiles (Special attack, training, or victory)
    if matches!(
        pet.action,
        PetAction::AttackSpecial { .. } | PetAction::VictoryHop
    ) {
        let fx_ox = if flip.is_flipped() {
            base_x + 20
        } else {
            base_x - 56
        };
        METAL_GREYMON_ATTACK_MISSILES.blit_to_band_scaled(
            buffer,
            band_y,
            fx_ox,
            body_oy + 40,
            SCALE,
            flip,
        );
    }
}

/// Blit WarGreymon's official Bandai 48x48 2-frame sprite and Gaia Force energy sphere.
fn render_wargreymon_anatomy(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2;

    let flip = if pet.tilt_deg > 0 || (pet.tilt_deg == 0 && pet.gaze_x > 0) {
        SpriteFlip::Flipped
    } else {
        SpriteFlip::Normal
    };

    let is_walk_step =
        matches!(pet.action, PetAction::PivotWalk { .. }) && (pet.anim_tick / 8 % 2 == 1);
    let walk_bob_y = if is_walk_step { -2 } else { 0 };

    let is_action = is_walk_step
        || matches!(
            pet.action,
            PetAction::HappyHop
                | PetAction::VictoryHop
                | PetAction::AttackSpecial { .. }
                | PetAction::MusicDance { .. }
                | PetAction::SuspendedKick
                | PetAction::ShakeChargeSprint { .. }
        );

    let body_sprite = if is_action {
        &WAR_GREYMON_BODY_ACTION
    } else {
        &WAR_GREYMON_BODY_NEUTRAL
    };

    let body_ox = base_x - 48;
    let body_oy = floor_y - 96 + walk_bob_y;
    let center_x = base_x;
    let center_y = body_oy + 48;

    if pet.tilt_deg != 0 {
        body_sprite.blit_to_band_rotated(
            buffer,
            band_y,
            (center_x, center_y),
            SCALE,
            flip,
            pet.tilt_deg,
        );
    } else {
        body_sprite.blit_to_band_scaled(buffer, band_y, body_ox, body_oy, SCALE, flip);
    }

    // Gaia Force Miniature Sun Energy Sphere (Held high overhead)
    if matches!(
        pet.action,
        PetAction::AttackSpecial { .. } | PetAction::VictoryHop
    ) {
        WAR_GREYMON_ATTACK_GAIA_FORCE.blit_to_band_scaled(
            buffer,
            band_y,
            base_x - 18,
            body_oy - 36,
            SCALE,
            flip,
        );
    }
}

/// Blit Omegamon's official Bandai 48x48 2-frame sprite and Supreme Cannon blast.
fn render_omegamon_anatomy(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2;

    let flip = if pet.tilt_deg > 0 || (pet.tilt_deg == 0 && pet.gaze_x > 0) {
        SpriteFlip::Flipped
    } else {
        SpriteFlip::Normal
    };

    let is_walk_step =
        matches!(pet.action, PetAction::PivotWalk { .. }) && (pet.anim_tick / 8 % 2 == 1);
    let walk_bob_y = if is_walk_step { -2 } else { 0 };

    let is_action = is_walk_step
        || matches!(
            pet.action,
            PetAction::HappyHop
                | PetAction::VictoryHop
                | PetAction::AttackSpecial { .. }
                | PetAction::MusicDance { .. }
                | PetAction::SuspendedKick
                | PetAction::ShakeChargeSprint { .. }
        );

    let body_sprite = if is_action {
        &OMEGAMON_BODY_ACTION
    } else {
        &OMEGAMON_BODY_NEUTRAL
    };

    let body_ox = base_x - 48;
    let body_oy = floor_y - 96 + walk_bob_y;
    let center_x = base_x;
    let center_y = body_oy + 48;

    if pet.tilt_deg != 0 {
        body_sprite.blit_to_band_rotated(
            buffer,
            band_y,
            (center_x, center_y),
            SCALE,
            flip,
            pet.tilt_deg,
        );
    } else {
        body_sprite.blit_to_band_scaled(buffer, band_y, body_ox, body_oy, SCALE, flip);
    }

    // Garuru Cannon Supreme Cannon Cryo-Energy Blast
    if matches!(
        pet.action,
        PetAction::AttackSpecial { .. } | PetAction::VictoryHop
    ) {
        let fx_ox = if flip.is_flipped() {
            base_x + 24
        } else {
            base_x - 64
        };
        OMEGAMON_ATTACK_SUPREME_CANNON.blit_to_band_scaled(
            buffer,
            band_y,
            fx_ox,
            body_oy + 32,
            SCALE,
            flip,
        );
    }
}

#[inline(never)]
fn render_food_and_poop(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    body_oy: i32,
    action: PetAction,
    poop_count: u8,
) {
    const SCALE: u8 = 2;
    if let PetAction::NoticeFood(food_type) | PetAction::EatingChew { food_type, .. } = action {
        let food_y = if matches!(action, PetAction::NoticeFood(_)) {
            body_oy - 28
        } else {
            body_oy + 44
        };
        match food_type {
            FoodType::Meat => {
                ITEM_ROAST_MEAT.blit_to_band_scaled(
                    buffer,
                    band_y,
                    base_x - 14,
                    food_y,
                    SCALE,
                    SpriteFlip::Normal,
                );
            }
            FoodType::Pill => {
                ITEM_PROTEIN_PILL.blit_to_band_scaled(
                    buffer,
                    band_y,
                    base_x - 10,
                    food_y,
                    SCALE,
                    SpriteFlip::Normal,
                );
            }
        }
    }
    if poop_count > 0 {
        ITEM_POOP.blit_to_band_scaled(buffer, band_y, base_x + 50, 150, SCALE, SpriteFlip::Normal);
    }
}

#[inline(never)]
fn render_flush_wave(buffer: &mut [u16], band_y: i32, tick: u8, anim_tick: u16) {
    let wave_front = ((tick as i32 * 240) / 20).clamp(0, 240);
    let wave_y_min = 148;
    let wave_y_max = 176;
    for py in wave_y_min..wave_y_max {
        if py >= band_y && py < band_y + BAND_HEIGHT as i32 {
            let row_idx = (py - band_y) as usize * 240;
            let start_x = (wave_front - 45).max(0);
            for px in start_x..wave_front {
                let col = if (px + py + anim_tick as i32) % 4 == 0 {
                    DIGIMON_16_PALETTE[2]
                } else if (px + py) % 2 == 0 {
                    DIGIMON_16_PALETTE[15]
                } else {
                    DIGIMON_16_PALETTE[12]
                };
                let target_idx = row_idx + px as usize;
                if target_idx < buffer.len() {
                    buffer[target_idx] = col;
                }
            }
        }
    }
}

#[inline(never)]
fn render_evolution_particles(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    progress_pct: u8,
    anim_tick: u16,
) {
    let colors = [
        DIGIMON_16_PALETTE[15],
        DIGIMON_16_PALETTE[7],
        DIGIMON_16_PALETTE[2],
        DIGIMON_16_PALETTE[12],
    ];
    for i in 0..6 {
        let spark_x = base_x - 45 + (i * 18);
        let phase = (anim_tick as i32 * 5 + i * 23) % 90;
        let spark_y = floor_y - phase;
        let col = colors[((i as usize) + (progress_pct as usize / 10)) % 4];

        for dy in 0..2 {
            let py = spark_y + dy;
            if py >= band_y && py < band_y + BAND_HEIGHT as i32 {
                let row_idx = (py - band_y) * 240;
                for dx in 0..2 {
                    let px = spark_x + dx;
                    if (0..240).contains(&px) {
                        let target_idx = (row_idx + px) as usize;
                        if target_idx < buffer.len() {
                            buffer[target_idx] = col;
                        }
                    }
                }
            }
        }
    }
}

/// Render action props (Meat, Pill, Poop, Hearts, Notes) with 2x scale.
fn render_props_and_particles(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2;
    let body_oy = floor_y - 72;

    render_food_and_poop(buffer, band_y, base_x, body_oy, pet.action, pet.poop_count);

    if let PetAction::FlushClean { tick } = pet.action {
        render_flush_wave(buffer, band_y, tick, pet.anim_tick);
    }

    // Sickness skull floating over head
    if pet.is_sick() {
        FX_SKULL.blit_to_band_scaled(
            buffer,
            band_y,
            base_x - 10,
            body_oy - 20,
            SCALE,
            SpriteFlip::Normal,
        );
    }

    // Healing bandage over body when HealCure action is active
    if matches!(pet.action, PetAction::HealCure { .. }) {
        FX_BANDAGE.blit_to_band_scaled(
            buffer,
            band_y,
            base_x - 10,
            body_oy + 10,
            SCALE,
            SpriteFlip::Normal,
        );
    }

    // Happy floating heart
    if pet.mood == PetMood::Happy || pet.pupil_shape == PupilShape::Heart {
        FX_HEART.blit_to_band_scaled(
            buffer,
            band_y,
            base_x + 40,
            body_oy - 12,
            SCALE,
            SpriteFlip::Normal,
        );
    }

    // Sleeping snot bubble & floating "Zzz"
    if let PetAction::SleepingZzz { bubble_pct } = pet.action {
        render_sleeping_indicators(buffer, band_y, base_x, body_oy, bubble_pct);
    } else if pet.is_sleeping() {
        render_sleeping_indicators(buffer, band_y, base_x, body_oy, 50);
    }

    // Dizzy stars floating above head
    if pet.pupil_shape == PupilShape::Dizzy {
        render_dizzy_indicators(buffer, band_y, base_x, body_oy, pet.anim_tick);
    }

    // Star sparkle above head
    if pet.pupil_shape == PupilShape::Star {
        render_star_sparkle(buffer, band_y, base_x, body_oy);
    }

    // Musical note floating when dancing
    if matches!(pet.action, PetAction::MusicDance { .. }) {
        let note_flip = if ((pet.anim_tick / 8) & 1) == 0 {
            SpriteFlip::Normal
        } else {
            SpriteFlip::Flipped
        };
        FX_NOTE.blit_to_band_scaled(buffer, band_y, base_x + 42, body_oy - 10, SCALE, note_flip);
    }

    // Digital matrix evolution particles during PetAction::Evolving
    if let PetAction::Evolving { progress_pct, .. } = pet.action {
        render_evolution_particles(buffer, band_y, base_x, floor_y, progress_pct, pet.anim_tick);
    }
}

fn draw_z_letter(buffer: &mut [u16], band_y: i32, x: i32, y: i32, color: u16) {
    const Z_PIXELS: [(i8, i8); 7] = [(0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2), (2, 2)];
    for (ox, oy) in Z_PIXELS {
        let py = y + oy as i32;
        let px = x + ox as i32;
        if py >= band_y && py < band_y + BAND_HEIGHT as i32 && (0..240).contains(&px) {
            buffer[(py - band_y) as usize * 240 + px as usize] = color;
        }
    }
}

fn draw_sparkle(buffer: &mut [u16], band_y: i32, x: i32, y: i32, color: u16) {
    const SPARKLE: [(i8, i8); 5] = [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)];
    for (ox, oy) in SPARKLE {
        let py = y + oy as i32;
        let px = x + ox as i32;
        if py >= band_y && py < band_y + BAND_HEIGHT as i32 && (0..240).contains(&px) {
            buffer[(py - band_y) as usize * 240 + px as usize] = color;
        }
    }
}

fn render_sleeping_indicators(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    body_oy: i32,
    bubble_pct: u8,
) {
    let cyan = DIGIMON_16_PALETTE[15];
    let white = DIGIMON_16_PALETTE[2];

    // 1. Pulsing breathing snot bubble near snout (base_x + 16, body_oy + 48)
    let bubble_radius = 2 + ((bubble_pct as i32 * 4) / 100);
    let bubble_cx = base_x + 16;
    let bubble_cy = body_oy + 48;
    for dy in -bubble_radius..=bubble_radius {
        let py = bubble_cy + dy;
        if py >= band_y && py < band_y + BAND_HEIGHT as i32 {
            let row_offset = (py - band_y) as usize * 240;
            for dx in -bubble_radius..=bubble_radius {
                let px = bubble_cx + dx;
                if (0..240).contains(&px) && dx * dx + dy * dy <= bubble_radius * bubble_radius {
                    let col = if dx == -1 && dy == -1 { white } else { cyan };
                    buffer[row_offset + px as usize] = col;
                }
            }
        }
    }

    // 2. Floating "Z" "z" "z" drifting upward to the right
    let sky_blue = DIGIMON_16_PALETTE[12];
    draw_z_letter(buffer, band_y, base_x + 28, body_oy - 4, sky_blue);
    draw_z_letter(buffer, band_y, base_x + 36, body_oy - 16, sky_blue);
    draw_z_letter(buffer, band_y, base_x + 44, body_oy - 28, sky_blue);
}

fn render_dizzy_indicators(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    body_oy: i32,
    anim_tick: u16,
) {
    let gold = DIGIMON_16_PALETTE[7];
    let orbit = ((anim_tick as i32 * 3) % 24) - 12;
    draw_sparkle(buffer, band_y, base_x - 16 + orbit, body_oy - 14, gold);
    draw_sparkle(buffer, band_y, base_x + 16 - orbit, body_oy - 20, gold);
}

fn render_star_sparkle(buffer: &mut [u16], band_y: i32, base_x: i32, body_oy: i32) {
    let gold = DIGIMON_16_PALETTE[7];
    draw_sparkle(buffer, band_y, base_x + 36, body_oy - 12, gold);
}

/// Render safe top HUD indicators: 4 Ruby Hearts (Hunger), 4 Golden Bolts (Strength), and Call Attention (⚠️).
fn render_stats_hud(
    target: &mut BandTarget,
    band_y: i32,
    hunger: u8,
    strength: u8,
    attention: AttentionState,
    anim_tick: u16,
) {
    // HUD is located in top safe band Y: 18..36
    if !(16..=38).contains(&band_y) && !(16..=38).contains(&(band_y + BAND_HEIGHT as i32)) {
        return;
    }

    // Left: 4 Hunger Hearts
    for i in 0..4 {
        let x = 70 + (i * 12);
        let color = if (i as u8) < hunger {
            COLOR_HUNGER_HEART
        } else {
            COLOR_EMPTY_SLOT
        };
        Rectangle::new(Point::new(x, 24), Size::new(8, 6))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)
            .ok();
    }

    // Right: 4 Strength Bolts
    for i in 0..4 {
        let x = 134 + (i * 12);
        let color = if (i as u8) < strength {
            COLOR_ENERGY_BOLT
        } else {
            COLOR_EMPTY_SLOT
        };
        Rectangle::new(Point::new(x, 24), Size::new(8, 6))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)
            .ok();
    }

    // Rightmost: Call for Attention blinking prompt icon (⚠️)
    if attention.is_calling() && ((anim_tick / 12) & 1) == 0 {
        HUD_CALL_ATTENTION.blit_to_band(
            &mut target.buffer[..],
            band_y,
            196,
            22,
            SpriteFlip::Normal,
        );
    }
}

/// Render retro Digivice Pendulum Training 10-swing energy gauge at bottom safe zone (Y: 184..196).
fn render_pendulum_training_hud(target: &mut BandTarget, band_y: i32, shakes: u8) {
    let band_bottom = band_y + BAND_HEIGHT as i32;
    if 198 < band_y || 182 >= band_bottom {
        return;
    }

    // Outer cyber gauge frame (X: 62..178, Y: 184..196, width 116, height 12)
    embedded_graphics::primitives::RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(62, 184), Size::new(116, 12)),
        Size::new(4, 4),
    )
    .into_styled(PrimitiveStyle::with_stroke(COLOR_PLATFORM_RIM, 1))
    .draw(target)
    .ok();

    // 10 Energy Segments (Quota = 10)
    for i in 0..10 {
        let x = 65 + (i * 11);
        let color = if (i as u8) < shakes {
            if shakes >= 10 {
                COLOR_EQ_CYAN
            } else {
                COLOR_EQ_GOLD
            }
        } else {
            COLOR_EMPTY_SLOT
        };
        Rectangle::new(Point::new(x, 186), Size::new(8, 8))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)
            .ok();
    }
}
