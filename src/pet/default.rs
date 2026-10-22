//! Cyber Cyclops Pet Behavior Engine
//!
//! Manages local pet moods, natural blinking, IMU gaze smoothing and reactions.

use super::{PetAnimationState, PetBehavior, PetEvent, PetFrame, PetMood, PupilShape};
use crate::domain::{FocusPhase, MusicMood, MusicScene};
use embassy_time::{Duration, Instant};

/// 16-step natural blink easing LUT (0 = closed, 100 = open, 105 = overshoot bounce)
pub const BLINK_LUT: [u8; 16] = [
    100, 75, 40, 10, 0, 0, 15, 50, 85, 105, 102, 100, 100, 100, 100, 100,
];

pub struct DefaultPet {
    animation: PetAnimationState,
    blink_tick: u16,
    blink_frame: Option<usize>,
    reaction_timer: Option<Instant>,
    focus_phase: FocusPhase,
}

impl Default for DefaultPet {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultPet {
    pub fn new() -> Self {
        Self {
            animation: PetAnimationState::default(),
            blink_tick: 0,
            blink_frame: None,
            reaction_timer: None,
            focus_phase: FocusPhase::Idle,
        }
    }

    /// Handle single tap (pat / pet head)
    pub fn on_tap(&mut self, now: Instant) {
        if self.animation.mood == PetMood::Sleeping {
            // Wake up on tap
            self.animation.mood = PetMood::Normal;
            self.animation.pupil_shape = PupilShape::Round;
            self.blink_frame = Some(6); // Wake up and open eyes
            self.reaction_timer = None;
        } else {
            // Enjoying patting! Happy eyes + heart pupil + smiling lift
            self.animation.mood = PetMood::Happy;
            self.animation.pupil_shape = PupilShape::Heart;
            self.animation.happy_lift_pct = 85;
            self.animation.affinity = self.animation.affinity.saturating_add(5);
            self.reaction_timer = Some(now);
        }
    }

    /// Handle double tap (feeding / exciting reward)
    pub fn on_double_tap(&mut self, now: Instant) {
        if self.animation.mood == PetMood::Sleeping {
            self.animation.mood = PetMood::Normal;
            self.blink_frame = Some(6);
        }
        self.animation.mood = PetMood::Happy;
        self.animation.pupil_shape = PupilShape::Star;
        self.animation.happy_lift_pct = 60;
        self.animation.energy = self.animation.energy.saturating_add(20).min(100);
        self.animation.affinity = self.animation.affinity.saturating_add(10);
        self.reaction_timer = Some(now);
    }

    /// Handle shake / high angular velocity (dizziness)
    pub fn on_shake(&mut self, now: Instant) {
        if self.animation.mood != PetMood::Sleeping {
            self.animation.mood = PetMood::Dizzy;
            self.animation.pupil_shape = PupilShape::Dizzy;
            self.animation.happy_lift_pct = 0;
            self.reaction_timer = Some(now);
        }
    }

    /// Put the pet to sleep when the device is placed face down.
    fn on_face_down(&mut self) {
        self.animation.mood = PetMood::Sleeping;
        self.animation.pupil_shape = PupilShape::Round;
        self.animation.happy_lift_pct = 0;
        self.blink_frame = None;
        self.reaction_timer = None;
    }

    /// Wake the pet after the device is returned face up.
    fn on_face_up(&mut self) {
        self.animation.mood = PetMood::Normal;
        self.animation.pupil_shape = PupilShape::Round;
        self.animation.happy_lift_pct = 0;
        self.blink_frame = Some(6);
    }

    /// Update gaze orientation from IMU accelerometer vector (zero-FPU fixed-point EMA)
    pub fn on_imu_attitude(&mut self, ax_mg: i32, ay_mg: i32) {
        if self.animation.mood == PetMood::Sleeping {
            self.animation.gaze_x = 0;
            self.animation.gaze_y = 0;
            return;
        }

        // Map +/-1000mg to +/-18px offset
        let target_x = ((ax_mg * 18) / 1000).clamp(-18, 18);
        let target_y = ((ay_mg * 18) / 1000).clamp(-18, 18);

        // Exponential moving average filter: new = (old * 3 + target) / 4
        self.animation.gaze_x = (((self.animation.gaze_x as i32 * 3) + target_x) / 4) as i8;
        self.animation.gaze_y = (((self.animation.gaze_y as i32 * 3) + target_y) / 4) as i8;
    }

    /// Map the local focus rhythm to a calm, non-host-dependent pet mood.
    pub fn on_focus_phase(&mut self, phase: FocusPhase) {
        self.focus_phase = phase;
        if self.animation.mood == PetMood::Sleeping || self.reaction_timer.is_some() {
            return;
        }
        match phase {
            FocusPhase::Focus => {
                self.animation.mood = PetMood::Focused;
                self.animation.pupil_shape = PupilShape::CatSlit;
            }
            FocusPhase::Idle
            | FocusPhase::ShortBreak
            | FocusPhase::LongBreak
            | FocusPhase::Paused
            | FocusPhase::Completed => {
                if self.animation.mood == PetMood::Focused {
                    self.animation.mood = PetMood::Normal;
                    self.animation.pupil_shape = PupilShape::Round;
                }
            }
        }
    }

    /// Apply a music scene as a low-priority emotional reaction.
    pub fn on_music_scene(&mut self, scene: MusicScene) {
        if self.animation.mood == PetMood::Sleeping || self.reaction_timer.is_some() {
            return;
        }
        self.animation.mood = match scene.mood {
            MusicMood::Happy | MusicMood::Excited if scene.energy >= 40 => PetMood::Happy,
            MusicMood::Focused => PetMood::Focused,
            MusicMood::Calm | MusicMood::Sad if scene.energy <= 20 => PetMood::Sleepy,
            _ => PetMood::Normal,
        };
        self.animation.pupil_shape = match self.animation.mood {
            PetMood::Focused => PupilShape::CatSlit,
            PetMood::Happy => PupilShape::Heart,
            PetMood::Sleepy => PupilShape::Round,
            _ => PupilShape::Round,
        };
    }

    /// Periodic behavior tick (drives blink frame, reaction timers, and UI snapshot)
    fn tick_frame(&mut self, now: Instant) -> PetFrame {
        self.animation.anim_tick = self.animation.anim_tick.wrapping_add(1);

        // 1. Check temporary interaction reaction timeout (3 seconds)
        if self
            .reaction_timer
            .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(3))
        {
            self.reaction_timer = None;
            if self.animation.mood != PetMood::Sleeping {
                if self.focus_phase == FocusPhase::Focus {
                    self.animation.mood = PetMood::Focused;
                    self.animation.pupil_shape = PupilShape::CatSlit;
                } else {
                    self.animation.mood = PetMood::Normal;
                    self.animation.pupil_shape = PupilShape::Round;
                }
                self.animation.happy_lift_pct = 0;
            }
        }

        // 2. Drive natural blinking animation
        let blink_pct = if self.animation.mood == PetMood::Sleeping {
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

        self.animation.blink_pct = blink_pct;
        self.animation.into()
    }
}

impl PetBehavior for DefaultPet {
    fn handle(&mut self, event: PetEvent, now: Instant) {
        match event {
            PetEvent::Tap => self.on_tap(now),
            PetEvent::DoubleTap => self.on_double_tap(now),
            PetEvent::Shake => self.on_shake(now),
            PetEvent::FaceDown => self.on_face_down(),
            PetEvent::FaceUp => self.on_face_up(),
            PetEvent::FocusPhase(phase) => self.on_focus_phase(phase),
            PetEvent::MusicScene(scene) => self.on_music_scene(scene),
            PetEvent::Attitude { ax_mg, ay_mg } => self.on_imu_attitude(ax_mg, ay_mg),
        }
    }

    fn tick(&mut self, now: Instant) -> PetFrame {
        self.tick_frame(now)
    }
}
