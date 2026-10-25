//! Modular Pet Asset Library.
//!
//! Organizes pet graphic assets into dedicated submodules by species name:
//! - `koromon`: 4bpp anatomy slices for Koromon
//! - (Future expansion: `agumon`, `greymon`, `metal_greymon`, `wargreymon`, `omegamom`)

pub mod agumon;
pub mod greymon;
pub mod koromon;
pub mod metal_greymon;
pub mod omegamon;
pub mod war_greymon;

pub use agumon::*;
pub use greymon::*;
pub use koromon::*;
pub use metal_greymon::*;
pub use omegamon::*;
pub use war_greymon::*;

/// Unified 16-color RGB565 palette for all 6 Digimon species (Koromon -> Omegamon),
/// accessories, and status indicators.
///
/// Multi-Species Shared Palette Allocations:
/// - 0: Transparent pass-through key (`0x0000`)
/// - 1: Outline Warm Burgundy (`0x3883`, soft hand-drawn pixel contour)
/// - 2: Highlight Pure White (`0xFFFF`, teeth, claws, Omegamon holy armor, eye sparkles)
/// - 3: Armor Steel Chrome (`0x9D56`, MetalGreymon helmet/arm, WarGreymon Dramon Killer, Omegamon blade)
/// - 4: Koromon Bubblegum Pink (`0xFCD4`, primary Koromon body)
/// - 5: Ruby Crimson / Flame Core (`0xF8C6`, Koromon eyes, fire breath core, WarGreymon hair, cape)
/// - 6: Deep Rose Shadow (`0xE2CE`, underbelly shading, mouth tongue)
/// - 7: Dinosaur Golden Yellow (`0xFEC0`, Agumon primary body, WarGreymon Digizoid armor, music sparks)
/// - 8: Deep Wine Red (`0x7822`, pupils, mouth depth, eye lashes / shadow)
/// - 9: Amber Orange (`0xF440`, Agumon shadow, Greymon primary body, Mega Flame body)
/// - 10: Emerald Green (`0x0668`, Agumon anime eyes, poop slime)
/// - 11: Deep Navy Blue (`0x118E`, Greymon tiger stripes, MetalGreymon wings, Garuru Cannon, headphones)
/// - 12: Sky Blue Highlight (`0x5D3F`, Garuru Cannon highlight, energy glow)
/// - 13: Horn Brown (`0x69E2`, Greymon skull helmet, roast meat crust)
/// - 14: Dark Gunmetal Grey (`0x4AA8`, MetalGreymon missile hatch, shadow steel)
/// - 15: Platform Tech Cyan (`0x06FD`, cyber pedestal rim line, digital evolution matrix sparks)
pub const DIGIMON_16_PALETTE: [u16; 16] = [
    0x0000, // 0: Transparent pass-through key
    0x3883, // 1: Outline Warm Burgundy
    0xFFFF, // 2: Highlight Pure White
    0x9D56, // 3: Armor Steel Chrome
    0xFCD4, // 4: Koromon Bubblegum Pink
    0xF8C6, // 5: Ruby Crimson / Flame Core
    0xE2CE, // 6: Deep Rose Shadow
    0xFEC0, // 7: Dinosaur Golden Yellow
    0x7822, // 8: Deep Wine Red
    0xF440, // 9: Amber Orange
    0x0668, // 10: Emerald Green
    0x118E, // 11: Deep Navy Blue
    0x5D3F, // 12: Sky Blue Highlight
    0x69E2, // 13: Horn Brown / Roast Meat Crust
    0x4AA8, // 14: Dark Gunmetal Grey
    0x06FD, // 15: Platform Tech Cyan
];

/// Horizontal flip orientation for sprite blitting, eliminating boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpriteFlip {
    #[default]
    Normal,
    Flipped,
}

impl SpriteFlip {
    #[inline]
    pub const fn is_flipped(&self) -> bool {
        matches!(self, Self::Flipped)
    }
}

/// Precomputed 91-entry Q12 sine LUT for fast zero-FPU rotation on RV32IMAC (0..=90 degrees, 4096 = 1.0)
pub const SIN_LUT_Q12: [i16; 91] = [
    0, 71, 143, 214, 286, 357, 428, 499, 570, 641, 711, 782, 852, 921, 991, 1060, 1129, 1198, 1266,
    1334, 1401, 1468, 1534, 1600, 1666, 1731, 1796, 1860, 1923, 1986, 2048, 2110, 2171, 2231, 2290,
    2349, 2408, 2465, 2522, 2578, 2633, 2687, 2741, 2793, 2845, 2896, 2946, 2996, 3044, 3091, 3138,
    3183, 3228, 3271, 3314, 3355, 3396, 3435, 3474, 3511, 3547, 3582, 3617, 3650, 3681, 3712, 3742,
    3770, 3798, 3824, 3849, 3873, 3896, 3917, 3937, 3956, 3974, 3991, 4006, 4021, 4034, 4046, 4056,
    4065, 4074, 4080, 4086, 4090, 4094, 4095, 4096,
];

/// Fast zero-FPU sine and cosine resolution in Q12 fixed point (4096 = 1.0) for any degree in [0, 360).
#[allow(
    clippy::large_stack_frames,
    reason = "91-entry static LUT lookup without heap allocation"
)]
#[inline]
pub const fn sin_cos_q12(deg: i16) -> (i32, i32) {
    let d = deg.rem_euclid(360) as usize;
    match d {
        0..=90 => (SIN_LUT_Q12[d] as i32, SIN_LUT_Q12[90 - d] as i32),
        91..=180 => (SIN_LUT_Q12[180 - d] as i32, -(SIN_LUT_Q12[d - 90] as i32)),
        181..=270 => (
            -(SIN_LUT_Q12[d - 180] as i32),
            -(SIN_LUT_Q12[270 - d] as i32),
        ),
        _ => (-(SIN_LUT_Q12[360 - d] as i32), SIN_LUT_Q12[d - 270] as i32),
    }
}

/// Compact 4-bits-per-pixel (4bpp) indexed color sprite slice.
/// Two horizontal pixels are packed per byte (high nibble = left pixel, low nibble = right pixel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSprite {
    pub width: u8,
    pub height: u8,
    pub palette: &'static [u16; 16],
    pub data: &'static [u8],
}

impl ColorSprite {
    /// Create a new sprite slice referencing the shared 16-color palette.
    /// Includes a const assertion ensuring the slice has enough packed bytes.
    pub const fn new(width: u8, height: u8, data: &'static [u8]) -> Self {
        let min_bytes = (width as usize * height as usize).div_ceil(2);
        assert!(
            data.len() >= min_bytes,
            "ColorSprite data slice length is shorter than required 4bpp nibble count"
        );
        Self {
            width,
            height,
            palette: &DIGIMON_16_PALETTE,
            data,
        }
    }

    /// Create a new sprite slice referencing a dedicated 16-color palette.
    pub const fn with_palette(
        width: u8,
        height: u8,
        palette: &'static [u16; 16],
        data: &'static [u8],
    ) -> Self {
        let min_bytes = (width as usize * height as usize).div_ceil(2);
        assert!(
            data.len() >= min_bytes,
            "ColorSprite data slice length is shorter than required 4bpp nibble count"
        );
        Self {
            width,
            height,
            palette,
            data,
        }
    }

    /// Blit this sprite slice directly into the 240x20 horizontal band buffer.
    ///
    /// Performance & Safety Invariants:
    /// 1. AABB 2D Clipping: Returns immediately in 0ns if sprite does not intersect this band or screen.
    /// 2. Bounded Horizontal Clipping: Clamps coordinates strictly within [0, 240).
    /// 3. Color Index 0 Transparency: Index 0 is treated as transparent and never overwrites target pixels.
    /// 4. Horizontal Flip Support: Allows dynamic direction turning without duplicate ROM assets.
    /// 5. Zero-Panic Bounds Protection: All slice accesses verify bounds explicitly.
    ///
    /// Blit 4bpp sprite with integer scaling directly to horizontal band buffer.
    pub fn blit_to_band_scaled(
        &self,
        band_buffer: &mut [u16],
        band_y_start: i32,
        pos_x: i32,
        pos_y: i32,
        scale: u8,
        flip: SpriteFlip,
    ) {
        const BAND_WIDTH: usize = 240;
        const BAND_HEIGHT: usize = 20;

        let s = scale.max(1) as i32;
        let sprite_w = self.width as i32;
        let sprite_h = self.height as i32;
        let scaled_w = sprite_w * s;
        let scaled_h = sprite_h * s;
        let band_y_end = band_y_start + BAND_HEIGHT as i32;

        // 1. AABB 2D Clipping: zero overhead for non-intersecting bands or off-screen sprites
        if pos_y + scaled_h <= band_y_start
            || pos_y >= band_y_end
            || pos_x + scaled_w <= 0
            || pos_x >= BAND_WIDTH as i32
        {
            return;
        }

        let y_start = pos_y.max(band_y_start);
        let y_end = (pos_y + scaled_h).min(band_y_end);

        for py in y_start..y_end {
            let r = (py - pos_y) / s;
            if r < 0 || r >= sprite_h {
                continue;
            }
            let target_row_idx = (py - band_y_start) as usize * BAND_WIDTH;
            let src_row_offset = (r * sprite_w) as usize;

            for c in 0..sprite_w {
                let pixel_idx = src_row_offset + c as usize;
                let byte_idx = pixel_idx / 2;
                if byte_idx >= self.data.len() {
                    continue;
                }

                let byte = self.data[byte_idx];
                let color_idx = if (pixel_idx & 1) == 0 {
                    (byte >> 4) & 0x0F
                } else {
                    byte & 0x0F
                };

                // Palette index 0 is transparent key
                if color_idx == 0 || (color_idx as usize) >= self.palette.len() {
                    continue;
                }
                let color = self.palette[color_idx as usize];

                let screen_c = if flip.is_flipped() {
                    sprite_w - 1 - c
                } else {
                    c
                };

                let px_start = pos_x + screen_c * s;
                for dx in 0..s {
                    let px = px_start + dx;
                    if (0..BAND_WIDTH as i32).contains(&px) {
                        let target_idx = target_row_idx + px as usize;
                        if target_idx < band_buffer.len() {
                            band_buffer[target_idx] = color;
                        }
                    }
                }
            }
        }
    }

    /// Blit 4bpp sprite with integer scaling and arbitrary angle rotation directly to horizontal band buffer.
    ///
    /// Performance & Zero-FPU Invariants:
    /// 1. Zero-FPU: Uses precomputed 91-entry Q12 sine LUT for instant trigonometric resolution.
    /// 2. Bounded Inverse Mapping: Computes `u_fixed` and `v_fixed` via two integer additions per pixel.
    /// 3. Fast-path: When `angle_deg == 0`, delegates to `blit_to_band_scaled` in 0ns.
    /// 4. AABB 2D Band Clipping: Instantly skips non-intersecting bands without rasterizing.
    /// 5. Color Index 0 Transparency: Index 0 is treated as transparent and never overwrites target pixels.
    #[allow(
        clippy::large_stack_frames,
        reason = "Zero-FPU fixed-point inverse rasterization into 240x20 band buffer"
    )]
    pub fn blit_to_band_rotated(
        &self,
        band_buffer: &mut [u16],
        band_y_start: i32,
        center: (i32, i32),
        scale: u8,
        flip: SpriteFlip,
        angle_deg: i16,
    ) {
        const BAND_WIDTH: usize = 240;
        const BAND_HEIGHT: usize = 20;

        let (center_x, center_y) = center;

        let s = scale.max(1) as i32;
        let sprite_w = self.width as i32;
        let sprite_h = self.height as i32;
        let scaled_w = sprite_w * s;
        let scaled_h = sprite_h * s;

        let norm_angle = angle_deg.rem_euclid(360);
        if norm_angle == 0 {
            let pos_x = center_x - scaled_w / 2;
            let pos_y = center_y - scaled_h / 2;
            self.blit_to_band_scaled(band_buffer, band_y_start, pos_x, pos_y, scale, flip);
            return;
        }

        // Bounding radius: half diagonal of scaled sprite box + 2px safety margin
        let r = (((scaled_w * scaled_w + scaled_h * scaled_h) as u32).isqrt() as i32) / 2 + 2;
        let band_y_end = band_y_start + BAND_HEIGHT as i32;

        let y_min = center_y - r;
        let y_max = center_y + r;
        if y_max <= band_y_start || y_min >= band_y_end {
            return;
        }

        let x_min = (center_x - r).max(0);
        let x_max = (center_x + r).min(BAND_WIDTH as i32);
        if x_min >= x_max {
            return;
        }

        let (sin_q12, cos_q12) = sin_cos_q12(norm_angle);
        let y_start = y_min.max(band_y_start);
        let y_end = y_max.min(band_y_end);

        for py in y_start..y_end {
            let dy = py - center_y;
            let target_row_idx = (py - band_y_start) as usize * BAND_WIDTH;

            let dx0 = x_min - center_x;
            let mut u_fixed = dx0 * cos_q12 + dy * sin_q12;
            let mut v_fixed = -dx0 * sin_q12 + dy * cos_q12;
            let du = cos_q12;
            let dv = -sin_q12;

            for px in x_min..x_max {
                let screen_u = (u_fixed >> 12) + scaled_w / 2;
                let screen_v = (v_fixed >> 12) + scaled_h / 2;

                u_fixed += du;
                v_fixed += dv;

                if screen_u < 0 || screen_u >= scaled_w || screen_v < 0 || screen_v >= scaled_h {
                    continue;
                }

                let unscaled_u = screen_u / s;
                let unscaled_v = screen_v / s;

                let c = if flip.is_flipped() {
                    sprite_w - 1 - unscaled_u
                } else {
                    unscaled_u
                };

                let pixel_idx = (unscaled_v * sprite_w + c) as usize;
                let byte_idx = pixel_idx / 2;
                if byte_idx >= self.data.len() {
                    continue;
                }

                let byte = self.data[byte_idx];
                let color_idx = if (pixel_idx & 1) == 0 {
                    (byte >> 4) & 0x0F
                } else {
                    byte & 0x0F
                };

                if color_idx == 0 || (color_idx as usize) >= self.palette.len() {
                    continue;
                }

                let target_idx = target_row_idx + px as usize;
                if target_idx < band_buffer.len() {
                    band_buffer[target_idx] = self.palette[color_idx as usize];
                }
            }
        }
    }

    /// Blit 1:1 without scaling (backwards-compatible alias for scale = 1)
    #[inline]
    pub fn blit_to_band(
        &self,
        band_buffer: &mut [u16],
        band_y_start: i32,
        pos_x: i32,
        pos_y: i32,
        flip: SpriteFlip,
    ) {
        self.blit_to_band_scaled(band_buffer, band_y_start, pos_x, pos_y, 1, flip);
    }
}

// -----------------------------------------------------------------------------
// Shared Accessories and Props
// -----------------------------------------------------------------------------

// Accessories: Retro Tech Headphones (32x10)
pub static PROP_HEADPHONES_DATA: [u8; 160] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x11, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0x11, 0x10, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x1B, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xB1, 0x10, 0x00, 0x00,
    0x00, 0x01, 0x1B, 0xB1, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1B, 0xB1, 0x10, 0x00,
    0x00, 0x1B, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0xB1, 0x00,
    0x01, 0xCC, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xCC, 0x10,
    0x01, 0xCC, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xCC, 0x10,
    0x01, 0xFF, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xFF, 0x10,
    0x01, 0xFF, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xFF, 0x10,
    0x00, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00,
];

pub static PROP_HEADPHONES: ColorSprite = ColorSprite::new(32, 10, &PROP_HEADPHONES_DATA);

// Accessories: Red Focus Headband (32x4)
pub static PROP_HEADBAND_DATA: [u8; 64] = [
    0x00, 0x00, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x00, 0x00,
    0x00, 0x01, 0x55, 0x55, 0x55, 0x55, 0x55, 0x51, 0x15, 0x55, 0x55, 0x55, 0x55, 0x55, 0x10, 0x00,
    0x00, 0x01, 0x55, 0x55, 0x55, 0x55, 0x55, 0x12, 0x21, 0x55, 0x55, 0x55, 0x55, 0x55, 0x10, 0x00,
    0x00, 0x00, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x00, 0x00,
];

pub static PROP_HEADBAND: ColorSprite = ColorSprite::new(32, 4, &PROP_HEADBAND_DATA);

// Item: Roast Meat with Bone (14x12)
pub static ITEM_ROAST_MEAT_DATA: [u8; 84] = [
    0x00, 0x00, 0x01, 0x11, 0x11, 0x11, 0x00, 0x00, 0x01, 0x1D, 0xDD, 0xDD, 0xDD, 0x10, 0x00, 0x1D,
    0xDD, 0xDD, 0xDD, 0xDD, 0xD1, 0x01, 0x9D, 0xDD, 0xDD, 0xDD, 0xDD, 0xD1, 0x19, 0x99, 0xDD, 0xDD,
    0xDD, 0xDD, 0x91, 0x19, 0x99, 0xDD, 0xDD, 0xDD, 0xDD, 0x91, 0x01, 0x9D, 0xDD, 0xDD, 0xDD, 0xDD,
    0xD1, 0x00, 0x1D, 0xDD, 0xDD, 0xDD, 0xDD, 0xD1, 0x00, 0x01, 0x1D, 0xDD, 0xDD, 0xDD, 0x10, 0x00,
    0x00, 0x01, 0x11, 0x11, 0x11, 0x00, 0x00, 0x00, 0x01, 0x22, 0x21, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x11, 0x00, 0x00, 0x00,
];

pub static ITEM_ROAST_MEAT: ColorSprite = ColorSprite::new(14, 12, &ITEM_ROAST_MEAT_DATA);

// Item: Slime Green Poop (12x12)
pub static ITEM_POOP_DATA: [u8; 72] = [
    0x00, 0x00, 0x01, 0x10, 0x00, 0x00, 0x00, 0x01, 0x1A, 0x11, 0x00, 0x00, 0x00, 0x1A, 0x2A, 0xAA,
    0x10, 0x00, 0x01, 0xAA, 0xAA, 0xAA, 0xA1, 0x00, 0x1A, 0xAA, 0xAA, 0xAA, 0xAA, 0x10, 0x1A, 0x2A,
    0xAA, 0xAA, 0xAA, 0x10, 0x1A, 0xAA, 0xAA, 0xAA, 0xAA, 0x10, 0x01, 0xAA, 0xAA, 0xAA, 0xAA, 0x10,
    0x1A, 0xAA, 0xAA, 0xAA, 0xAA, 0xA1, 0x1A, 0x2A, 0xAA, 0xAA, 0xAA, 0xA1, 0x01, 0x1A, 0xAA, 0xAA,
    0xAA, 0x10, 0x00, 0x01, 0x11, 0x11, 0x11, 0x00,
];

pub static ITEM_POOP: ColorSprite = ColorSprite::new(12, 12, &ITEM_POOP_DATA);

// Effect: Pink Love Heart (10x8)
pub static FX_HEART_DATA: [u8; 40] = [
    0x01, 0x10, 0x00, 0x01, 0x10, 0x14, 0x41, 0x00, 0x14, 0x41, 0x14, 0x44, 0x11, 0x44, 0x41, 0x14,
    0x44, 0x44, 0x44, 0x41, 0x01, 0x44, 0x44, 0x44, 0x10, 0x00, 0x14, 0x44, 0x41, 0x00, 0x00, 0x01,
    0x44, 0x10, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00,
];

pub static FX_HEART: ColorSprite = ColorSprite::new(10, 8, &FX_HEART_DATA);

// Effect: Golden Musical Note (8x10)
pub static FX_NOTE_DATA: [u8; 40] = [
    0x00, 0x01, 0x11, 0x00, 0x00, 0x01, 0x77, 0x10, 0x00, 0x01, 0x71, 0x00, 0x00, 0x01, 0x71, 0x00,
    0x00, 0x01, 0x71, 0x00, 0x00, 0x01, 0x71, 0x00, 0x01, 0x11, 0x71, 0x00, 0x17, 0x77, 0x10, 0x00,
    0x17, 0x77, 0x10, 0x00, 0x01, 0x11, 0x00, 0x00,
];

pub static FX_NOTE: ColorSprite = ColorSprite::new(8, 10, &FX_NOTE_DATA);

// Item: Super Vitamin Protein Pill (10x10)
pub static ITEM_PROTEIN_PILL_DATA: [u8; 50] = [
    0x00, 0x11, 0x11, 0x11, 0x00, // Row 0
    0x01, 0x22, 0x2C, 0xCC, 0x10, // Row 1
    0x12, 0x22, 0x2C, 0xFC, 0xC1, // Row 2
    0x12, 0x22, 0x2C, 0xCC, 0xC1, // Row 3
    0x12, 0x22, 0x2C, 0xCC, 0xC1, // Row 4
    0x12, 0x22, 0x2C, 0xCC, 0xC1, // Row 5
    0x12, 0x22, 0x2C, 0xCC, 0xC1, // Row 6
    0x13, 0x33, 0x3B, 0xBB, 0xB1, // Row 7
    0x01, 0x33, 0x3B, 0xBB, 0x10, // Row 8
    0x00, 0x11, 0x11, 0x11, 0x00, // Row 9
];

pub static ITEM_PROTEIN_PILL: ColorSprite = ColorSprite::new(10, 10, &ITEM_PROTEIN_PILL_DATA);

// Effect: Sickness Skull (10x10)
pub static FX_SKULL_DATA: [u8; 50] = [
    0x01, 0x11, 0x11, 0x11, 0x10, // Row 0
    0x12, 0x22, 0x22, 0x22, 0x21, // Row 1
    0x12, 0x88, 0x22, 0x88, 0x21, // Row 2
    0x12, 0x88, 0x22, 0x88, 0x21, // Row 3
    0x12, 0x22, 0x88, 0x22, 0x21, // Row 4
    0x01, 0x22, 0x22, 0x22, 0x10, // Row 5
    0x01, 0x21, 0x21, 0x21, 0x10, // Row 6
    0x01, 0x22, 0x22, 0x22, 0x10, // Row 7
    0x00, 0x11, 0x11, 0x11, 0x00, // Row 8
    0x00, 0x00, 0x00, 0x00, 0x00, // Row 9
];

pub static FX_SKULL: ColorSprite = ColorSprite::new(10, 10, &FX_SKULL_DATA);

// Effect: Healing Bandage (10x10)
pub static FX_BANDAGE_DATA: [u8; 50] = [
    0x00, 0x11, 0x00, 0x11, 0x00, // Row 0
    0x01, 0x77, 0x11, 0x77, 0x10, // Row 1
    0x17, 0x72, 0x77, 0x27, 0x71, // Row 2
    0x17, 0x27, 0x77, 0x72, 0x71, // Row 3
    0x01, 0x17, 0x77, 0x71, 0x10, // Row 4
    0x01, 0x17, 0x77, 0x71, 0x10, // Row 5
    0x17, 0x27, 0x77, 0x72, 0x71, // Row 6
    0x17, 0x72, 0x77, 0x27, 0x71, // Row 7
    0x01, 0x77, 0x11, 0x77, 0x10, // Row 8
    0x00, 0x11, 0x00, 0x11, 0x00, // Row 9
];

pub static FX_BANDAGE: ColorSprite = ColorSprite::new(10, 10, &FX_BANDAGE_DATA);

// HUD: Call for Attention Indicator (10x10)
pub static HUD_CALL_ATTENTION_DATA: [u8; 50] = [
    0x00, 0x00, 0x77, 0x00, 0x00, // Row 0
    0x00, 0x07, 0x55, 0x70, 0x00, // Row 1
    0x00, 0x75, 0x55, 0x57, 0x00, // Row 2
    0x07, 0x55, 0x22, 0x55, 0x70, // Row 3
    0x07, 0x55, 0x22, 0x55, 0x70, // Row 4
    0x75, 0x55, 0x22, 0x55, 0x57, // Row 5
    0x75, 0x55, 0x00, 0x55, 0x57, // Row 6
    0x75, 0x55, 0x22, 0x55, 0x57, // Row 7
    0x07, 0x77, 0x77, 0x77, 0x70, // Row 8
    0x00, 0x01, 0x11, 0x10, 0x00, // Row 9
];

pub static HUD_CALL_ATTENTION: ColorSprite = ColorSprite::new(10, 10, &HUD_CALL_ATTENTION_DATA);
