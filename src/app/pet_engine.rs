//! Cyber Cyclops Pet Behavior Engine
//!
//! Manages pet moods, natural blinking animations, IMU gaze smoothing,
//! and reaction lifecycles (patting, feeding, shake-dizziness, sleeping).

use crate::ui::{PetMood, PetUiState, PupilShape};
use embassy_time::{Duration, Instant};

/// 16-step natural blink easing LUT (0 = closed, 100 = open, 105 = overshoot bounce)
pub const BLINK_LUT: [u8; 16] = [
    100, 75, 40, 10, 0, 0, 15, 50, 85, 105, 102, 100, 100, 100, 100, 100,
];

pub use crate::proto::FaceDownTransition;

pub struct PetEngine {
    mood: PetMood,
    pupil_shape: PupilShape,
    gaze_x: i8,
    gaze_y: i8,
    blink_tick: u16,
    blink_frame: Option<usize>,
    reaction_timer: Option<Instant>,
    anim_tick: u16,
    energy: u8,
    affinity: u16,
    happy_lift_pct: u8,
}

impl Default for PetEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PetEngine {
    pub fn new() -> Self {
        Self {
            mood: PetMood::Normal,
            pupil_shape: PupilShape::Round,
            gaze_x: 0,
            gaze_y: 0,
            blink_tick: 0,
            blink_frame: None,
            reaction_timer: None,
            anim_tick: 0,
            energy: 100,
            affinity: 0,
            happy_lift_pct: 0,
        }
    }

    /// Handle single tap (pat / pet head)
    pub fn on_tap(&mut self, now: Instant) {
        if self.mood == PetMood::Sleeping {
            // Wake up on tap
            self.mood = PetMood::Normal;
            self.pupil_shape = PupilShape::Round;
            self.blink_frame = Some(6); // Wake up and open eyes
            self.reaction_timer = None;
        } else {
            // Enjoying patting! Happy eyes + heart pupil + smiling lift
            self.mood = PetMood::Happy;
            self.pupil_shape = PupilShape::Heart;
            self.happy_lift_pct = 85;
            self.affinity = self.affinity.saturating_add(5);
            self.reaction_timer = Some(now);
        }
    }

    /// Handle double tap (feeding / exciting reward)
    pub fn on_double_tap(&mut self, now: Instant) {
        if self.mood == PetMood::Sleeping {
            self.mood = PetMood::Normal;
            self.blink_frame = Some(6);
        }
        self.mood = PetMood::Happy;
        self.pupil_shape = PupilShape::Star;
        self.happy_lift_pct = 60;
        self.energy = self.energy.saturating_add(20).min(100);
        self.affinity = self.affinity.saturating_add(10);
        self.reaction_timer = Some(now);
    }

    /// Handle shake / high angular velocity (dizziness)
    pub fn on_shake(&mut self, now: Instant) {
        if self.mood != PetMood::Sleeping {
            self.mood = PetMood::Dizzy;
            self.pupil_shape = PupilShape::Dizzy;
            self.happy_lift_pct = 0;
            self.reaction_timer = Some(now);
        }
    }

    /// Handle facedown transition with strong typing (invert to sleep, restore to wake)
    pub fn on_facedown(&mut self, transition: FaceDownTransition) {
        match transition {
            FaceDownTransition::Entering => {
                self.mood = PetMood::Sleeping;
                self.pupil_shape = PupilShape::Round;
                self.happy_lift_pct = 0;
                self.blink_frame = None;
                self.reaction_timer = None;
            }
            FaceDownTransition::Exiting => {
                self.mood = PetMood::Normal;
                self.pupil_shape = PupilShape::Round;
                self.happy_lift_pct = 0;
                self.blink_frame = Some(6);
            }
        }
    }

    /// Update gaze orientation from IMU accelerometer vector (zero-FPU fixed-point EMA)
    pub fn on_imu_attitude(&mut self, ax_mg: i32, ay_mg: i32) {
        if self.mood == PetMood::Sleeping {
            self.gaze_x = 0;
            self.gaze_y = 0;
            return;
        }

        // Map +/-1000mg to +/-18px offset
        let target_x = ((ax_mg * 18) / 1000).clamp(-18, 18);
        let target_y = ((ay_mg * 18) / 1000).clamp(-18, 18);

        // Exponential moving average filter: new = (old * 3 + target) / 4
        self.gaze_x = (((self.gaze_x as i32 * 3) + target_x) / 4) as i8;
        self.gaze_y = (((self.gaze_y as i32 * 3) + target_y) / 4) as i8;
    }

    /// Update telemetry reactions (CPU load maps to work focus)
    pub fn on_telemetry(&mut self, cpu_pct: u8, _cpu_temp: i8) {
        // Only override if no temporary user interaction is active
        if self.reaction_timer.is_none() && self.mood != PetMood::Sleeping {
            if cpu_pct > 75 {
                self.mood = PetMood::Focused;
                self.pupil_shape = PupilShape::CatSlit;
            } else if self.mood == PetMood::Focused {
                self.mood = PetMood::Normal;
                self.pupil_shape = PupilShape::Round;
            }
        }
    }

    /// Periodic behavior tick (drives blink frame, reaction timers, and UI snapshot)
    pub fn tick(&mut self, now: Instant) -> PetUiState {
        self.anim_tick = self.anim_tick.wrapping_add(1);

        // 1. Check temporary interaction reaction timeout (3 seconds)
        if self
            .reaction_timer
            .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(3))
        {
            self.reaction_timer = None;
            if self.mood != PetMood::Sleeping {
                self.mood = PetMood::Normal;
                self.pupil_shape = PupilShape::Round;
                self.happy_lift_pct = 0;
            }
        }

        // 2. Drive natural blinking animation
        let blink_pct = if self.mood == PetMood::Sleeping {
            0
        } else if let Some(idx) = self.blink_frame {
            let pct = BLINK_LUT[idx];
            if idx + 1 < BLINK_LUT.len() {
                self.blink_frame = Some(idx + 1);
            } else {
                self.blink_frame = None;
            }
            pct
        } else {
            self.blink_tick = self.blink_tick.saturating_add(1);
            // Trigger a blink every ~140 ticks (~3.5 seconds)
            if self.blink_tick >= 140 {
                self.blink_tick = 0;
                self.blink_frame = Some(0);
            }
            100
        };

        PetUiState {
            mood: self.mood,
            pupil_shape: self.pupil_shape,
            gaze_x: self.gaze_x,
            gaze_y: self.gaze_y,
            blink_pct,
            happy_lift_pct: self.happy_lift_pct,
            anim_tick: self.anim_tick,
            energy: self.energy,
            affinity: self.affinity,
        }
    }
}
