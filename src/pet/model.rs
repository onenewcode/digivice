//! Rendering-neutral pet model types.
//!
//! A pet implementation emits a `PetFrame`; the display renderer only consumes this
//! snapshot. Keeping the frame outside `ui` lets users replace the pet behavior without
//! rewriting the application coordinator or hardware drivers.

use super::species::{ActionPhase, PetAction, PetSpecies, SleepState};
use super::transition::SquashStretch;
use crate::domain::ChargeState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PetMood {
    #[default]
    Normal,
    Happy,
    Focused,
    Dizzy,
    Sleepy,
    Sleeping,
    Shocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PupilShape {
    #[default]
    Round,
    CatSlit,
    Heart,
    Star,
    Dizzy,
    Lightning,
    Cross,
}

/// Authoritative mutable animation state owned by a pet behavior.
#[allow(clippy::large_stack_frames)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetAnimationState {
    pub species: PetSpecies,
    pub action: PetAction,
    pub action_phase: ActionPhase,
    pub squash: SquashStretch,
    pub hunger: u8,
    pub strength: u8,
    pub poop_count: u8,
    pub sleep_state: SleepState,
    pub charge_state: ChargeState,
    pub music_beat_pulse: u8,

    // Common facial expression and mood metrics
    pub mood: PetMood,
    pub pupil_shape: PupilShape,
    pub gaze_x: i8,
    pub gaze_y: i8,
    pub tilt_deg: i16,
    pub blink_pct: u8,
    pub happy_lift_pct: u8,
    pub anim_tick: u16,
    pub energy: u8,
    pub affinity: u16,
    pub wander_x: i8,
}

impl PetAnimationState {
    #[inline]
    pub fn is_sleeping(&self) -> bool {
        self.sleep_state.is_sleeping()
    }

    #[inline]
    pub fn is_charging(&self) -> bool {
        self.charge_state.is_charging()
    }
}

impl Default for PetAnimationState {
    fn default() -> Self {
        Self {
            species: PetSpecies::Koromon,
            action: PetAction::IdleBreathe,
            action_phase: ActionPhase::Loop,
            squash: SquashStretch::default(),
            hunger: 4,
            strength: 4,
            poop_count: 0,
            sleep_state: SleepState::Awake,
            charge_state: ChargeState::StandbyOrDone,
            music_beat_pulse: 0,
            mood: PetMood::Normal,
            pupil_shape: PupilShape::Round,
            gaze_x: 0,
            gaze_y: 0,
            tilt_deg: 0,
            blink_pct: 100,
            happy_lift_pct: 0,
            anim_tick: 0,
            energy: 100,
            affinity: 0,
            wander_x: 0,
        }
    }
}

/// Immutable animation snapshot consumed by the UI renderer.
#[allow(clippy::large_stack_frames)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetFrame {
    pub species: PetSpecies,
    pub action: PetAction,
    pub action_phase: ActionPhase,
    pub squash: SquashStretch,
    pub hunger: u8,
    pub strength: u8,
    pub poop_count: u8,
    pub sleep_state: SleepState,
    pub charge_state: ChargeState,
    pub music_beat_pulse: u8,

    pub mood: PetMood,
    pub pupil_shape: PupilShape,
    pub gaze_x: i8,
    pub gaze_y: i8,
    pub tilt_deg: i16,
    pub blink_pct: u8,
    pub happy_lift_pct: u8,
    pub anim_tick: u16,
    pub energy: u8,
    pub affinity: u16,
    pub wander_x: i8,
}

impl PetFrame {
    #[inline]
    pub fn is_sleeping(&self) -> bool {
        self.sleep_state.is_sleeping()
    }

    #[inline]
    pub fn is_charging(&self) -> bool {
        self.charge_state.is_charging()
    }
}

impl From<PetAnimationState> for PetFrame {
    fn from(state: PetAnimationState) -> Self {
        Self {
            species: state.species,
            action: state.action,
            action_phase: state.action_phase,
            squash: state.squash,
            hunger: state.hunger,
            strength: state.strength,
            poop_count: state.poop_count,
            sleep_state: state.sleep_state,
            charge_state: state.charge_state,
            music_beat_pulse: state.music_beat_pulse,
            mood: state.mood,
            pupil_shape: state.pupil_shape,
            gaze_x: state.gaze_x,
            gaze_y: state.gaze_y,
            tilt_deg: state.tilt_deg,
            blink_pct: state.blink_pct,
            happy_lift_pct: state.happy_lift_pct,
            anim_tick: state.anim_tick,
            energy: state.energy,
            affinity: state.affinity,
            wander_x: state.wander_x,
        }
    }
}

impl Default for PetFrame {
    fn default() -> Self {
        PetAnimationState::default().into()
    }
}
