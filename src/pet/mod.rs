//! Replaceable pet behavior layer.
//!
//! Product behavior talks to a small event/frame contract instead of a concrete mascot.
//! A future pet can implement `PetBehavior` and be supplied to
//! `AppStateMachine::with_pet` without changing BLE, focus, storage or display code.

pub mod default;
mod model;

pub use default::DefaultPet;
pub use model::{PetAnimationState, PetFrame, PetMood, PupilShape};

use crate::domain::{FocusPhase, MusicScene};
use embassy_time::Instant;

/// Events understood by a pet implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetEvent {
    Tap,
    DoubleTap,
    Shake,
    FaceDown,
    FaceUp,
    FocusPhase(FocusPhase),
    MusicScene(MusicScene),
    Attitude { ax_mg: i32, ay_mg: i32 },
}

/// Stable extension point for replaceable pets.
pub trait PetBehavior {
    /// Apply a local interaction or sensor-derived event.
    fn handle(&mut self, event: PetEvent, now: Instant);

    /// Advance animation and return the next render frame.
    fn tick(&mut self, now: Instant) -> PetFrame;
}
