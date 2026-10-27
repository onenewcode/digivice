//! Dirty rectangle bounds and band mapping calculation for GC9A01 partial flushing.

use crate::pet::PetAction;
use crate::ui::UiState;

pub const BAND_HEIGHT: usize = 20;
pub const NUM_BANDS: usize = 12;
pub const DISPLAY_HEIGHT: u16 = 240;

/// Map pixel Y interval `[y_min, y_max]` to inclusive band indices `[start_band, end_band]`.
#[inline]
pub fn band_range_for_y_bounds(y_min: u16, y_max: u16) -> (usize, usize) {
    let start_band = (y_min as usize) / BAND_HEIGHT;
    let end_band = ((y_max as usize) / BAND_HEIGHT).min(NUM_BANDS - 1);
    (start_band, end_band)
}

/// Calculate vertical dirty pixel bounds `[y_min, y_max]` for partial band flushing.
#[inline]
pub fn pet_dirty_y_bounds(state: &UiState) -> (u16, u16) {
    if state.music.bpm > 0 || state.pet.music_beat_pulse > 0 {
        // Full screen outer circular neon audio visualizer ring (Y: 8..232 spans Band 0 through Band 11)
        (0, 239)
    } else if state.pet.tilt_deg != 0
        || state.pet.training_shakes > 0
        || matches!(state.pet.action, PetAction::ShakeChargeSprint { .. })
    {
        // Encompasses Top HUD (Y: 18..36) down through bottom Pendulum Training HUD (Y: 184..196)
        // and rotating character + platform (Y: 35..195), spanning Bands 0..=9 (10 bands, rows 0..=199).
        // Bands 10 and 11 (rows 200..=239) are completely empty and skipped (16.7% bandwidth reduction).
        (18, 196)
    } else {
        // Neutral posture: Top HUD (Y: 18..36), character with lifted ears (Y: 45..164),
        // and cyber platform base (Y: 160..178).
        // Spans Bands 0..=8 (exactly 9 bands, rows 0..=179).
        // Bands 9, 10, 11 (rows 180..=239, 60 rows) are completely empty and skipped (25% SPI bandwidth reduction).
        (18, 178)
    }
}

/// Expand dirty pixel bounds with previous frame to cleanly wipe ghost artifacts.
#[inline]
pub fn expand_dirty_y(current: (u16, u16), previous: (u16, u16)) -> (u16, u16) {
    (current.0.min(previous.0), current.1.max(previous.1))
}
