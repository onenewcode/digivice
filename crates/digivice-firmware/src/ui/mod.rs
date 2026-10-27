//! Circular UI Rendering Engine for Waveshare 240x240 Round LCD

pub mod pet_view;
pub mod selftest_view;
pub mod status_view;
pub mod theme;

pub use digivice_core::ui::dirty;
pub use digivice_core::ui::state;
pub use digivice_core::ui::{UiState, band_range_for_y_bounds, expand_dirty_y, pet_dirty_y_bounds};

pub use pet_view::render_pet;
pub use selftest_view::render_selftest;
pub use status_view::render_status;
pub use theme::{
    CENTER_X, CENTER_Y, COLOR_AMBER, COLOR_BG, COLOR_CAPSULE_BG, COLOR_CAPSULE_BORDER, COLOR_CYAN,
    COLOR_DARK_GRAY, COLOR_GRAY, COLOR_GREEN, COLOR_RED, COLOR_WHITE, RADIUS,
};

// Re-exports for convenience across UI submodules
pub use digivice_bsp::ButtonPressState;
pub use digivice_core::domain::{DeviceOrientation, InvalidViewMode, ViewMode};
pub use digivice_core::pet::{PetMood, PupilShape};
pub use digivice_core::protocol::BleBlip;
