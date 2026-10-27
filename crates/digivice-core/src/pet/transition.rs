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

    /// Move one quarter of the remaining distance, snapping to the target below four units.
    ///
    /// For tilt angle, implements shortest-path circular interpolation across 360 degrees.
    pub fn update(&mut self, target_x: i16, target_y: i16, target_tilt: i16) {
        let dx = i32::from(target_x) - i32::from(self.current_x);
        let dy = i32::from(target_y) - i32::from(self.current_y);
        self.current_x = (i32::from(self.current_x) + ema_step(dx)) as i16;
        self.current_y = (i32::from(self.current_y) + ema_step(dy)) as i16;

        let diff = (target_tilt as i32 - self.current_tilt_deg as i32).rem_euclid(360);
        let shortest_diff = if diff > 180 { diff - 360 } else { diff };
        let next_deg = (self.current_tilt_deg as i32 + ema_step(shortest_diff)).rem_euclid(360);
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

#[inline]
fn ema_step(delta: i32) -> i32 {
    let step = delta / 4;
    // Integer division must not leave a permanent one-to-three-unit residual.
    if step == 0 { delta } else { step }
}

#[cfg(test)]
mod tests {
    use super::InertialTransform;

    fn angular_distance(from: i16, to: i16) -> i32 {
        let distance = (i32::from(to) - i32::from(from)).abs() % 360;
        distance.min(360 - distance)
    }

    #[test]
    fn small_position_errors_reach_the_exact_target() {
        for current in -8..=8 {
            for target in -8..=8 {
                let mut transform = InertialTransform::new();
                transform.reset(current, -current, 0);
                for _ in 0..16 {
                    transform.update(target, -target, 0);
                }
                assert_eq!(transform.current_x, target);
                assert_eq!(transform.current_y, -target);
            }
        }
    }

    #[test]
    fn extreme_positions_converge_without_overshoot() {
        for (start, target) in [
            (i16::MIN, i16::MAX),
            (i16::MAX, i16::MIN),
            (i16::MIN, 0),
            (i16::MAX, 0),
        ] {
            let mut transform = InertialTransform::new();
            transform.reset(start, start, 0);
            for _ in 0..64 {
                let previous = transform.current_x;
                transform.update(target, target, 0);
                assert!(
                    (previous.min(target)..=previous.max(target)).contains(&transform.current_x)
                );
                assert_eq!(transform.current_y, transform.current_x);
                if previous != target {
                    assert_ne!(transform.current_x, previous);
                }
            }
            assert_eq!(transform.current_x, target);
        }
    }

    #[test]
    fn every_initial_tilt_returns_exactly_to_neutral() {
        for initial in i16::MIN..=i16::MAX {
            let mut transform = InertialTransform::new();
            transform.reset(0, 0, initial);
            for _ in 0..32 {
                transform.update(0, 0, 0);
            }
            assert_eq!(transform.current_tilt_deg, 0, "initial tilt: {initial}");
        }
    }

    #[test]
    fn all_normalized_tilt_pairs_converge_without_stalling() {
        for initial in -180..=180 {
            for target in -180..=180 {
                let mut transform = InertialTransform::new();
                transform.reset(0, 0, initial);
                for _ in 0..32 {
                    let previous_distance = angular_distance(transform.current_tilt_deg, target);
                    transform.update(0, 0, target);
                    assert!((-179..=180).contains(&transform.current_tilt_deg));
                    let distance = angular_distance(transform.current_tilt_deg, target);
                    if previous_distance == 0 {
                        assert_eq!(distance, 0);
                    } else {
                        assert!(distance < previous_distance);
                    }
                }
                assert_eq!(angular_distance(transform.current_tilt_deg, target), 0);
            }
        }
    }

    #[test]
    fn tilt_uses_the_shortest_arc_in_both_directions() {
        for (initial, target, expected) in [
            (170, -170, 175),
            (-170, 170, -175),
            (179, -179, -179),
            (-179, 179, 179),
            (0, 180, 45),
            (0, -180, 45),
        ] {
            let mut transform = InertialTransform::new();
            transform.reset(0, 0, initial);
            transform.update(0, 0, target);
            assert_eq!(transform.current_tilt_deg, expected);
        }
    }
}
