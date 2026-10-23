//! Smooth motion transition engine: Squash & Stretch damping and Zero-FPU inertial smoothing.

/// Dynamic volumetric compensation representing soft jelly-like elasticity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SquashStretch {
    /// Horizontal scale percentage (100 = neutral, 115 = expanded width)
    pub scale_x_pct: u8,
    /// Vertical scale percentage (100 = neutral, 85 = squashed height)
    pub scale_y_pct: u8,
    /// Vertical origin offset compensation in pixels
    pub offset_y_px: i8,
}

impl Default for SquashStretch {
    fn default() -> Self {
        Self {
            scale_x_pct: 100,
            scale_y_pct: 100,
            offset_y_px: 0,
        }
    }
}

impl SquashStretch {
    /// Create a SquashStretch snapshot from a spring damping step index.
    #[inline]
    pub const fn from_spring_step(step: usize) -> Self {
        let (dy, dx) = spring_squash_step(step);
        Self {
            scale_x_pct: (100i8 + dx) as u8,
            scale_y_pct: (100i8 + dy) as u8,
            offset_y_px: dy,
        }
    }
}

/// 8-step precomputed spring damping table (height_delta, width_delta).
/// Models elastic impact, rebound, and settlement without runtime floating-point physics.
pub const SPRING_SQUASH_LUT: [(i8, i8); 8] = [
    (-3, 3), // Step 0: Impact compression (squashed flat)
    (-2, 2), // Step 1: Absorbing shock
    (2, -2), // Step 2: Spring rebound stretch (elongated)
    (1, -1), // Step 3: Peak rebound
    (-1, 1), // Step 4: Secondary small compression
    (1, -1), // Step 5: Secondary rebound
    (0, 0),  // Step 6: Settling
    (0, 0),  // Step 7: Fully neutral
];

/// Safe, saturating lookup for the spring squash table (returns (0,0) once settled).
#[inline]
pub const fn spring_squash_step(step: usize) -> (i8, i8) {
    if step < SPRING_SQUASH_LUT.len() {
        SPRING_SQUASH_LUT[step]
    } else {
        (0, 0)
    }
}

/// Zero-FPU exponential moving average (EMA) filter for inertial position and tilt tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InertialTransform {
    pub current_x: i16,
    pub current_y: i16,
    pub current_tilt_deg: i16,
}

impl InertialTransform {
    pub const fn new() -> Self {
        Self {
            current_x: 0,
            current_y: 0,
            current_tilt_deg: 0,
        }
    }

    /// Update position and tilt using integer fixed-point EMA:
    /// `new = (old * 3 + target) / 4`
    ///
    /// For tilt angle, implements shortest-path circular interpolation across 360 degrees.
    pub fn update(&mut self, target_x: i16, target_y: i16, target_tilt: i16) {
        self.current_x = (((self.current_x as i32 * 3) + target_x as i32) / 4) as i16;
        self.current_y = (((self.current_y as i32 * 3) + target_y as i32) / 4) as i16;

        let diff = (target_tilt as i32 - self.current_tilt_deg as i32).rem_euclid(360);
        let shortest_diff = if diff > 180 { diff - 360 } else { diff };
        let next_deg = (self.current_tilt_deg as i32 + shortest_diff / 4).rem_euclid(360);
        self.current_tilt_deg = if next_deg > 180 {
            (next_deg - 360) as i16
        } else {
            next_deg as i16
        };
    }

    /// Reset instantly to neutral coordinates without gliding.
    pub fn reset(&mut self, x: i16, y: i16, tilt: i16) {
        self.current_x = x;
        self.current_y = y;
        self.current_tilt_deg = tilt;
    }
}
