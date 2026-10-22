//! Soft mascot renderer for the circular 240x240 display.
//!
//! The pet view intentionally avoids dashboard chrome: a rounded body, expressive eyes,
//! cheeks and small decorative motion make the device read as a companion at a glance.
//! Geometry is integer-only so the renderer remains inexpensive on the ESP32-C6.

use super::{CENTER_X, UiState};
use crate::pet::{PetMood, PupilShape};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{
    Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle, Triangle,
};

const PET_BG: Rgb565 = Rgb565::new(31, 54, 23);
const FUR: Rgb565 = Rgb565::new(31, 38, 21);
const FUR_LIGHT: Rgb565 = Rgb565::new(31, 49, 28);
const FUR_SHADOW: Rgb565 = Rgb565::new(24, 26, 16);
const OUTLINE: Rgb565 = Rgb565::new(8, 10, 8);
const EYE_WHITE: Rgb565 = Rgb565::new(31, 61, 31);
const PUPIL: Rgb565 = Rgb565::new(3, 5, 5);
const PINK: Rgb565 = Rgb565::new(31, 25, 25);
const PINK_DARK: Rgb565 = Rgb565::new(23, 12, 15);
const MINT: Rgb565 = Rgb565::new(7, 43, 24);
const SUN: Rgb565 = Rgb565::new(31, 49, 8);
const SKY: Rgb565 = Rgb565::new(8, 43, 30);

/// Render the replaceable pet frame as a soft, expressive mascot.
pub fn render_pet<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: &UiState) {
    let pet = &state.pet;
    Rectangle::new(Point::new(0, 0), Size::new(240, 240))
        .into_styled(PrimitiveStyle::with_fill(PET_BG))
        .draw(target)
        .ok();

    render_sparkles(target, pet.mood, pet.anim_tick);

    // A tiny bob keeps the body alive without moving the face out of the safe area.
    let bob = if pet.happy_lift_pct > 0 {
        -(((pet.anim_tick / 3) % 4) as i32)
    } else {
        0
    };
    render_ears(target, bob);
    render_body(target, bob, pet.mood);

    let gaze_x = (pet.gaze_x as i32 / 2).clamp(-9, 9);
    let gaze_y = (pet.gaze_y as i32 / 3).clamp(-6, 6);
    let eye_y = 112 + bob + gaze_y;
    render_eye(
        target,
        Point::new(88 + gaze_x, eye_y),
        pet.pupil_shape,
        pet.blink_pct,
        pet.mood,
    );
    render_eye(
        target,
        Point::new(152 + gaze_x, eye_y),
        pet.pupil_shape,
        pet.blink_pct,
        pet.mood,
    );
    render_cheeks(target, bob, pet.mood);
    render_mouth(target, bob, pet.mood);
    render_status_charm(target, pet.mood, pet.energy, pet.anim_tick);
}

fn render_ears<D: DrawTarget<Color = Rgb565>>(target: &mut D, bob: i32) {
    let left = Triangle::new(
        Point::new(48, 88 + bob),
        Point::new(67, 36 + bob),
        Point::new(101, 84 + bob),
    );
    left.into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(FUR)
            .stroke_color(OUTLINE)
            .stroke_width(3)
            .build(),
    )
    .draw(target)
    .ok();
    Triangle::new(
        Point::new(61, 77 + bob),
        Point::new(68, 52 + bob),
        Point::new(87, 78 + bob),
    )
    .into_styled(PrimitiveStyle::with_fill(PINK))
    .draw(target)
    .ok();

    Triangle::new(
        Point::new(139, 84 + bob),
        Point::new(173, 36 + bob),
        Point::new(192, 88 + bob),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(FUR)
            .stroke_color(OUTLINE)
            .stroke_width(3)
            .build(),
    )
    .draw(target)
    .ok();
    Triangle::new(
        Point::new(153, 78 + bob),
        Point::new(172, 52 + bob),
        Point::new(179, 77 + bob),
    )
    .into_styled(PrimitiveStyle::with_fill(PINK))
    .draw(target)
    .ok();
}

fn render_body<D: DrawTarget<Color = Rgb565>>(target: &mut D, bob: i32, mood: PetMood) {
    let body = Rectangle::new(Point::new(40, 72 + bob), Size::new(160, 132));
    RoundedRectangle::with_equal_corners(body, Size::new(42, 42))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(FUR)
                .stroke_color(OUTLINE)
                .stroke_width(3)
                .build(),
        )
        .draw(target)
        .ok();

    // Belly patch and two soft paws keep the silhouette toy-like instead of mechanical.
    RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(74, 157 + bob), Size::new(92, 54)),
        Size::new(22, 22),
    )
    .into_styled(PrimitiveStyle::with_fill(FUR_LIGHT))
    .draw(target)
    .ok();
    for x in [58, 158] {
        Circle::new(Point::new(x, 182 + bob), 32)
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(FUR_LIGHT)
                    .stroke_color(OUTLINE)
                    .stroke_width(2)
                    .build(),
            )
            .draw(target)
            .ok();
    }

    if mood == PetMood::Sleeping {
        // A muted shadow grounds the sleeping body.
        Rectangle::new(Point::new(72, 205), Size::new(96, 5))
            .into_styled(PrimitiveStyle::with_fill(FUR_SHADOW))
            .draw(target)
            .ok();
    }
}

fn render_eye<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    center: Point,
    shape: PupilShape,
    blink_pct: u8,
    mood: PetMood,
) {
    let closed = blink_pct <= 18 || mood == PetMood::Sleeping;
    if closed {
        Line::new(center + Point::new(-13, 0), center + Point::new(-5, 3))
            .into_styled(PrimitiveStyle::with_stroke(PUPIL, 3))
            .draw(target)
            .ok();
        Line::new(center + Point::new(-5, 3), center + Point::new(5, 3))
            .into_styled(PrimitiveStyle::with_stroke(PUPIL, 3))
            .draw(target)
            .ok();
        Line::new(center + Point::new(5, 3), center + Point::new(13, 0))
            .into_styled(PrimitiveStyle::with_stroke(PUPIL, 3))
            .draw(target)
            .ok();
        return;
    }

    let eye_radius = if mood == PetMood::Shocked { 20 } else { 16 };
    Circle::new(
        center - Point::new(eye_radius, eye_radius),
        (eye_radius * 2) as u32,
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(EYE_WHITE)
            .stroke_color(OUTLINE)
            .stroke_width(2)
            .build(),
    )
    .draw(target)
    .ok();

    render_pupil(target, center, shape, mood);

    // A partial blink is represented by a warm lid sliding over the lower eye.
    if blink_pct < 100 {
        let cover_height = ((100 - blink_pct.min(100)) as i32 * eye_radius / 100).max(0);
        if cover_height > 0 {
            Rectangle::new(
                center + Point::new(-eye_radius - 1, eye_radius - cover_height),
                Size::new((eye_radius * 2 + 2) as u32, cover_height as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(FUR))
            .draw(target)
            .ok();
        }
    }
}

fn render_pupil<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    center: Point,
    shape: PupilShape,
    mood: PetMood,
) {
    match shape {
        PupilShape::CatSlit => {
            RoundedRectangle::with_equal_corners(
                Rectangle::new(center + Point::new(-4, -11), Size::new(8, 22)),
                Size::new(4, 4),
            )
            .into_styled(PrimitiveStyle::with_fill(PUPIL))
            .draw(target)
            .ok();
        }
        PupilShape::Heart => {
            Circle::new(center + Point::new(-10, -7), 13)
                .into_styled(PrimitiveStyle::with_fill(PINK_DARK))
                .draw(target)
                .ok();
            Circle::new(center + Point::new(-1, -7), 13)
                .into_styled(PrimitiveStyle::with_fill(PINK_DARK))
                .draw(target)
                .ok();
            Triangle::new(
                center + Point::new(-11, 0),
                center + Point::new(11, 0),
                center + Point::new(0, 12),
            )
            .into_styled(PrimitiveStyle::with_fill(PINK_DARK))
            .draw(target)
            .ok();
        }
        PupilShape::Star => {
            Triangle::new(
                center + Point::new(-11, 3),
                center + Point::new(11, 3),
                center + Point::new(0, -12),
            )
            .into_styled(PrimitiveStyle::with_fill(SUN))
            .draw(target)
            .ok();
            Triangle::new(
                center + Point::new(-11, -3),
                center + Point::new(11, -3),
                center + Point::new(0, 12),
            )
            .into_styled(PrimitiveStyle::with_fill(SUN))
            .draw(target)
            .ok();
        }
        PupilShape::Dizzy => {
            Line::new(center + Point::new(-9, -9), center + Point::new(9, 9))
                .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 3))
                .draw(target)
                .ok();
            Line::new(center + Point::new(-9, 9), center + Point::new(9, -9))
                .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 3))
                .draw(target)
                .ok();
        }
        PupilShape::Lightning => {
            Triangle::new(
                center + Point::new(1, -12),
                center + Point::new(-8, 1),
                center + Point::new(2, 1),
            )
            .into_styled(PrimitiveStyle::with_fill(SUN))
            .draw(target)
            .ok();
            Triangle::new(
                center + Point::new(2, 0),
                center + Point::new(-2, 12),
                center + Point::new(9, -2),
            )
            .into_styled(PrimitiveStyle::with_fill(SUN))
            .draw(target)
            .ok();
        }
        PupilShape::Cross => {
            Line::new(center + Point::new(-9, -9), center + Point::new(9, 9))
                .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 3))
                .draw(target)
                .ok();
            Line::new(center + Point::new(-9, 9), center + Point::new(9, -9))
                .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 3))
                .draw(target)
                .ok();
        }
        PupilShape::Round => {
            Circle::new(center - Point::new(8, 8), 16)
                .into_styled(PrimitiveStyle::with_fill(PUPIL))
                .draw(target)
                .ok();
            Circle::new(center + Point::new(-5, -6), 6)
                .into_styled(PrimitiveStyle::with_fill(EYE_WHITE))
                .draw(target)
                .ok();
            if mood == PetMood::Happy {
                Circle::new(center + Point::new(4, 5), 3)
                    .into_styled(PrimitiveStyle::with_fill(EYE_WHITE))
                    .draw(target)
                    .ok();
            }
        }
    }
}

fn render_cheeks<D: DrawTarget<Color = Rgb565>>(target: &mut D, bob: i32, mood: PetMood) {
    let cheek_color = if matches!(mood, PetMood::Sleeping | PetMood::Sleepy) {
        PINK_DARK
    } else {
        PINK
    };
    for x in [61, 165] {
        Circle::new(Point::new(x, 143 + bob), 20)
            .into_styled(PrimitiveStyle::with_fill(cheek_color))
            .draw(target)
            .ok();
        Line::new(Point::new(x + 4, 146 + bob), Point::new(x + 12, 142 + bob))
            .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 1))
            .draw(target)
            .ok();
    }
}

fn render_mouth<D: DrawTarget<Color = Rgb565>>(target: &mut D, bob: i32, mood: PetMood) {
    let nose = Point::new(CENTER_X, 139 + bob);
    Triangle::new(
        nose + Point::new(-5, 0),
        nose + Point::new(5, 0),
        nose + Point::new(0, 5),
    )
    .into_styled(PrimitiveStyle::with_fill(PINK_DARK))
    .draw(target)
    .ok();
    if mood == PetMood::Sleeping {
        Line::new(
            Point::new(CENTER_X - 10, 151 + bob),
            Point::new(CENTER_X + 10, 151 + bob),
        )
        .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 2))
        .draw(target)
        .ok();
    } else {
        Line::new(
            Point::new(CENTER_X, 144 + bob),
            Point::new(CENTER_X, 150 + bob),
        )
        .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 2))
        .draw(target)
        .ok();
        Line::new(
            Point::new(CENTER_X, 150 + bob),
            Point::new(CENTER_X - 9, 147 + bob),
        )
        .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 2))
        .draw(target)
        .ok();
        Line::new(
            Point::new(CENTER_X, 150 + bob),
            Point::new(CENTER_X + 9, 147 + bob),
        )
        .into_styled(PrimitiveStyle::with_stroke(PINK_DARK, 2))
        .draw(target)
        .ok();
    }
}

fn render_sparkles<D: DrawTarget<Color = Rgb565>>(target: &mut D, mood: PetMood, tick: u16) {
    let color = match mood {
        PetMood::Happy => SUN,
        PetMood::Focused => SKY,
        PetMood::Dizzy => PINK,
        PetMood::Sleeping | PetMood::Sleepy => MINT,
        _ => FUR_LIGHT,
    };
    let pulse = (tick / 8) % 3;
    for (x, y) in [(28, 54), (210, 62), (29, 175), (211, 178)] {
        let arm = 3 + pulse as i32;
        Line::new(Point::new(x - arm, y), Point::new(x + arm, y))
            .into_styled(PrimitiveStyle::with_stroke(color, 2))
            .draw(target)
            .ok();
        Line::new(Point::new(x, y - arm), Point::new(x, y + arm))
            .into_styled(PrimitiveStyle::with_stroke(color, 2))
            .draw(target)
            .ok();
    }
}

fn render_status_charm<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    mood: PetMood,
    energy: u8,
    tick: u16,
) {
    let color = match mood {
        PetMood::Happy => PINK,
        PetMood::Focused => SKY,
        PetMood::Sleeping => MINT,
        _ => SUN,
    };
    let y = 30 + ((tick / 10) % 2) as i32;
    Circle::new(Point::new(112, y), 9)
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(target)
        .ok();
    Circle::new(Point::new(120, y), 9)
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(target)
        .ok();
    Triangle::new(
        Point::new(112, y + 4),
        Point::new(128, y + 4),
        Point::new(120, y + 17),
    )
    .into_styled(PrimitiveStyle::with_fill(color))
    .draw(target)
    .ok();

    // A tiny energy bar is a status hint, not a dashboard.
    let width = (energy.min(100) as u32 * 34) / 100;
    Rectangle::new(Point::new(103, 219), Size::new(34, 4))
        .into_styled(PrimitiveStyle::with_fill(FUR_SHADOW))
        .draw(target)
        .ok();
    Rectangle::new(Point::new(103, 219), Size::new(width, 4))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(target)
        .ok();
}
