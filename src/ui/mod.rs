//! Circular UI Rendering Engine for Waveshare 240x240 Round LCD

pub mod pet_view;
pub mod selftest_view;
pub mod state;
pub mod theme;

pub use state::UiState;
pub use theme::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_BG, COLOR_CAPSULE_BG, COLOR_CAPSULE_BORDER, COLOR_CYAN,
    COLOR_DARK_GRAY, COLOR_GRAY, COLOR_GREEN, COLOR_RED, COLOR_WHITE, RADIUS,
};

// Re-exports for convenience across UI submodules
pub use crate::board::ButtonPressState;
pub use crate::domain::{DeviceOrientation, InvalidViewMode, ViewMode};
pub use crate::pet::{PetMood, PupilShape};
pub use crate::radio::BleBlip;
