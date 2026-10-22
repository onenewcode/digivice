//! Circular LCD dimensions and RGB565 theme palette.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

pub const CENTER_X: i32 = 120;
pub const CENTER_Y: i32 = 120;
pub const RADIUS: i32 = 118;

// Dark Geek High-Contrast RGB565 Theme Palette
pub const COLOR_BG: Rgb565 = Rgb565::new(1, 2, 2); // #080C14 Deep dark space
pub const COLOR_CAPSULE_BG: Rgb565 = Rgb565::new(2, 8, 6); // #162032
pub const COLOR_CAPSULE_BORDER: Rgb565 = Rgb565::new(7, 47, 31); // #38BDF8 Cyan blue
pub const COLOR_GREEN: Rgb565 = Rgb565::new(2, 46, 16); // #10B981 Emerald green
pub const COLOR_CYAN: Rgb565 = Rgb565::new(0, 45, 26); // #06B6D4 Cyan
pub const COLOR_AMBER: Rgb565 = Rgb565::new(30, 39, 1); // #F59E0B Amber orange
pub const COLOR_RED: Rgb565 = Rgb565::new(29, 17, 8); // #EF4444 Coral red
pub const COLOR_WHITE: Rgb565 = Rgb565::WHITE;
pub const COLOR_GRAY: Rgb565 = Rgb565::new(12, 25, 12); // Medium gray
pub const COLOR_DARK_GRAY: Rgb565 = Rgb565::new(6, 12, 6); // Dim grid lines
