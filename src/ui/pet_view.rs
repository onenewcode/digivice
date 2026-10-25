//! Full-Color Koromon Mascot & Circular Audio Visualizer Renderer.
//!
//! Replaces legacy vector math with high-performance 4bpp sprite blitting,
//! dynamic Squash & Stretch positioning, modular anatomy assembly, and outer neon EQ ring.

use super::UiState;
use crate::board::display::{BAND_HEIGHT, BandTarget};
use crate::pet::assets::{
    AGUMON_ATTACK_FIRE, AGUMON_BODY_ACTION, AGUMON_BODY_NEUTRAL, DIGIMON_16_PALETTE, FX_HEART,
    FX_NOTE, GREYMON_ATTACK_MEGA_FLAME, GREYMON_BODY_ACTION, GREYMON_BODY_NEUTRAL, ITEM_POOP,
    ITEM_ROAST_MEAT, KOROMON_BODY_ACTION, KOROMON_BODY_NEUTRAL, METAL_GREYMON_ATTACK_MISSILES,
    METAL_GREYMON_BODY_ACTION, METAL_GREYMON_BODY_NEUTRAL, OMEGAMON_ATTACK_SUPREME_CANNON,
    OMEGAMON_BODY_ACTION, OMEGAMON_BODY_NEUTRAL, PROP_HEADBAND, PROP_HEADPHONES,
    WAR_GREYMON_ATTACK_GAIA_FORCE, WAR_GREYMON_BODY_ACTION, WAR_GREYMON_BODY_NEUTRAL,
};
use crate::pet::species::{MusicDanceStyle, PetAction, PetSpecies};
use crate::pet::{PetMood, SpriteFlip};
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

/// Calculate vertical dirty pixel bounds `[y_min, y_max]` for partial band flushing.
#[inline]
pub fn pet_dirty_y_bounds(state: &UiState) -> (u16, u16) {
    if state.music.bpm > 0 || state.pet.music_beat_pulse > 0 || state.pet.tilt_deg != 0 {
        // Full screen outer audio visualizer ring or dynamic tilt rotation across bands
        (0, 239)
    } else if state.pet.training_shakes > 0
        || matches!(state.pet.action, PetAction::ShakeChargeSprint { .. })
    {
        // Encompasses Top HUD down through bottom Pendulum Training HUD (Y: 18..204)
        (18, 204)
    } else {
        // Encompasses Top HUD (Y: 18..34), Koromon with lifted ears (Y: 45..164),
        // and cyber platform base (Y: 160..178)
        (18, 180)
    }
}

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

    // 6. Draw Top Stats HUD (Hunger Hearts & Strength Bolts) as topmost overlay
    render_stats_hud(target, band_y, pet.hunger, pet.strength);

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

    let is_walk_step = pet.wander_x != 0 && (pet.anim_tick / 8 % 2 == 1);
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
                (center_x, center_y),
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
                (center_x, center_y),
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

    let is_walk_step = pet.wander_x != 0 && (pet.anim_tick / 8 % 2 == 1);
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

    let is_walk_step = pet.wander_x != 0 && (pet.anim_tick / 8 % 2 == 1);
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

    let is_walk_step = pet.wander_x != 0 && (pet.anim_tick / 8 % 2 == 1);
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

    let is_walk_step = pet.wander_x != 0 && (pet.anim_tick / 8 % 2 == 1);
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

    let is_walk_step = pet.wander_x != 0 && (pet.anim_tick / 8 % 2 == 1);
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

/// Render action props (Meat, Poop, Hearts, Notes) with 2x scale.
fn render_props_and_particles(
    buffer: &mut [u16],
    band_y: i32,
    base_x: i32,
    floor_y: i32,
    pet: &crate::pet::PetFrame,
) {
    const SCALE: u8 = 2;
    let body_oy = floor_y - 72;

    // Roast meat dropping or being eaten
    if matches!(
        pet.action,
        PetAction::NoticeFood | PetAction::EatingChew { .. }
    ) {
        let meat_y = if matches!(pet.action, PetAction::NoticeFood) {
            body_oy - 28 // Meat hovering/falling from above
        } else {
            body_oy + 44 // Meat near mouth fangs
        };
        ITEM_ROAST_MEAT.blit_to_band_scaled(
            buffer,
            band_y,
            base_x - 14,
            meat_y,
            SCALE,
            SpriteFlip::Normal,
        );
    }

    // Accumulated green poop on the floor
    if pet.poop_count > 0 {
        ITEM_POOP.blit_to_band_scaled(buffer, band_y, base_x + 50, 150, SCALE, SpriteFlip::Normal);
    }

    // Happy floating heart
    if pet.mood == PetMood::Happy {
        FX_HEART.blit_to_band_scaled(
            buffer,
            band_y,
            base_x + 40,
            body_oy - 12,
            SCALE,
            SpriteFlip::Normal,
        );
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
        let colors = [
            DIGIMON_16_PALETTE[15], // Tech Cyan
            DIGIMON_16_PALETTE[7],  // Digizoid Gold
            DIGIMON_16_PALETTE[2],  // Pure White
            DIGIMON_16_PALETTE[12], // Sky Blue
        ];
        // 6 vertical energy stream pillars rising from floor_y
        for i in 0..6 {
            let spark_x = base_x - 45 + (i * 18);
            let phase = (pet.anim_tick as i32 * 5 + i * 23) % 90;
            let spark_y = floor_y - phase;
            let col = colors[((i as usize) + (progress_pct as usize / 10)) % 4];

            // 2x2 particle
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
}

/// Render safe top HUD indicators: 4 Ruby Hearts (Hunger) and 4 Golden Bolts (Strength).
fn render_stats_hud(target: &mut BandTarget, band_y: i32, hunger: u8, strength: u8) {
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
