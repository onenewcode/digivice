//! Replaceable pet behavior layer.
//!
//! Product behavior talks to a small event/frame contract instead of a concrete mascot.
//! A future pet can implement `PetBehavior` and be supplied to
//! `AppStateMachine::with_pet` without changing BLE, storage or display code.

pub mod assets;
pub mod default;
mod model;
pub mod species;
pub mod sprites;
pub mod transition;

pub use assets::*;
pub use default::DefaultPet;
pub use model::{PetAnimationState, PetFrame, PetMood, PupilShape};
pub use species::{
    ActionPhase, AttentionState, FoodType, HealthStatus, LookDirection, MusicDanceStyle, PetAction,
    PetSpecies, SleepState, StepLeg,
};
pub use transition::{InertialTransform, SPRING_SQUASH_LUT, SquashStretch, spring_squash_step};

use crate::domain::{ChargeState, MusicScene};
use embassy_time::Instant;

/// Events understood by a pet implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetEvent {
    Tap,
    DoubleTap,
    Shake,
    Attack,
    Flush,
    FaceDown,
    FaceUp,
    MusicScene(MusicScene),
    Attitude { ax_mg: i32, ay_mg: i32, az_mg: i32 },
    ChargeState(ChargeState),
    CircadianHour(u8),
}

/// Stable extension point for replaceable pets.
pub trait PetBehavior {
    /// Apply a local interaction or sensor-derived event.
    fn handle(&mut self, event: PetEvent, now: Instant);

    /// Advance animation and return the next render frame.
    fn tick(&mut self, now: Instant) -> PetFrame;
}
