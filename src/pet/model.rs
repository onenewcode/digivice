//! Rendering-neutral pet model types.
//!
//! A pet implementation emits a `PetFrame`; the display renderer only consumes this
//! snapshot. Keeping the frame outside `ui` lets users replace the pet behavior without
//! rewriting the application coordinator or hardware drivers.

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetAnimationState {
    pub mood: PetMood,
    pub pupil_shape: PupilShape,
    pub gaze_x: i8,
    pub gaze_y: i8,
    pub blink_pct: u8,
    pub happy_lift_pct: u8,
    pub anim_tick: u16,
    pub energy: u8,
    pub affinity: u16,
}

impl Default for PetAnimationState {
    fn default() -> Self {
        Self {
            mood: PetMood::Normal,
            pupil_shape: PupilShape::Round,
            gaze_x: 0,
            gaze_y: 0,
            blink_pct: 100,
            happy_lift_pct: 0,
            anim_tick: 0,
            energy: 100,
            affinity: 0,
        }
    }
}

/// Immutable animation snapshot consumed by the UI renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetFrame {
    pub mood: PetMood,
    pub pupil_shape: PupilShape,
    pub gaze_x: i8,
    pub gaze_y: i8,
    pub blink_pct: u8,
    pub happy_lift_pct: u8,
    pub anim_tick: u16,
    pub energy: u8,
    pub affinity: u16,
}

impl From<PetAnimationState> for PetFrame {
    fn from(state: PetAnimationState) -> Self {
        Self {
            mood: state.mood,
            pupil_shape: state.pupil_shape,
            gaze_x: state.gaze_x,
            gaze_y: state.gaze_y,
            blink_pct: state.blink_pct,
            happy_lift_pct: state.happy_lift_pct,
            anim_tick: state.anim_tick,
            energy: state.energy,
            affinity: state.affinity,
        }
    }
}

impl Default for PetFrame {
    fn default() -> Self {
        PetAnimationState::default().into()
    }
}
