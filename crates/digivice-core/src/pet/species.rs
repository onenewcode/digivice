//! Species definitions, action vocabulary, and behavioral states for the Pet System.

/// Evolution species catalog. Covers the canonical Tai Kamiya Digimon Adventure evolution line:
/// Koromon -> Agumon -> Greymon -> MetalGreymon -> WarGreymon -> Omegamon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PetSpecies {
    #[default]
    Koromon, // Stage 0: In-Training II
    Agumon,       // Stage 1: Rookie
    Greymon,      // Stage 2: Champion
    MetalGreymon, // Stage 3: Ultimate
    WarGreymon,   // Stage 4: Mega
    Omegamon,     // Stage 5: Ultra
}

impl PetSpecies {
    /// Convert species to 0-based monotonic evolution stage index (0..=5).
    #[inline]
    pub const fn stage_index(&self) -> u8 {
        match self {
            Self::Koromon => 0,
            Self::Agumon => 1,
            Self::Greymon => 2,
            Self::MetalGreymon => 3,
            Self::WarGreymon => 4,
            Self::Omegamon => 5,
        }
    }

    /// Construct species from 0-based evolution stage index (clamped to 5).
    #[inline]
    pub const fn from_stage(stage: u8) -> Self {
        match stage {
            0 => Self::Koromon,
            1 => Self::Agumon,
            2 => Self::Greymon,
            3 => Self::MetalGreymon,
            4 => Self::WarGreymon,
            _ => Self::Omegamon,
        }
    }

    /// Canonical display name of the Digimon species.
    #[inline]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Koromon => "Koromon",
            Self::Agumon => "Agumon",
            Self::Greymon => "Greymon",
            Self::MetalGreymon => "MetalGreymon",
            Self::WarGreymon => "WarGreymon",
            Self::Omegamon => "Omegamon",
        }
    }

    /// Evolution tier label.
    #[inline]
    pub const fn tier_name(&self) -> &'static str {
        match self {
            Self::Koromon => "In-Training II",
            Self::Agumon => "Rookie",
            Self::Greymon => "Champion",
            Self::MetalGreymon => "Ultimate",
            Self::WarGreymon => "Mega",
            Self::Omegamon => "Ultra",
        }
    }

    /// Next sequential evolution species in the line (cycles at Omegamon).
    #[inline]
    pub const fn next_evolution(&self) -> Self {
        match self {
            Self::Koromon => Self::Agumon,
            Self::Agumon => Self::Greymon,
            Self::Greymon => Self::MetalGreymon,
            Self::MetalGreymon => Self::WarGreymon,
            Self::WarGreymon => Self::Omegamon,
            Self::Omegamon => Self::Koromon,
        }
    }

    /// Previous devolution species.
    #[inline]
    pub const fn prev_evolution(&self) -> Self {
        match self {
            Self::Koromon => Self::Omegamon,
            Self::Agumon => Self::Koromon,
            Self::Greymon => Self::Agumon,
            Self::MetalGreymon => Self::Greymon,
            Self::WarGreymon => Self::MetalGreymon,
            Self::Omegamon => Self::WarGreymon,
        }
    }

    /// Whether this species can evolve to a higher tier.
    #[inline]
    pub const fn has_next_evolution(&self) -> bool {
        !matches!(self, Self::Omegamon)
    }

    /// Base standard weight in grams for the species.
    #[inline]
    pub const fn base_weight_g(&self) -> u8 {
        match self {
            Self::Koromon => 5,
            Self::Agumon => 10,
            Self::Greymon => 20,
            Self::MetalGreymon => 30,
            Self::WarGreymon => 40,
            Self::Omegamon => 50,
        }
    }
}

/// Nurturing food item categories: Roast Meat (Hunger) vs. Super Vitamin Protein Pill (Strength).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoodType {
    #[default]
    Meat,
    Pill,
}

/// Three-phase lifecycle for every composite action.
/// Ensures graceful anticipation (Enter) and settlement (Exit) without abrupt pops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionPhase {
    #[default]
    Enter,
    Loop,
    Exit,
}

/// Strongly-typed sleep state eliminating boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SleepState {
    #[default]
    Awake,
    Asleep,
}

impl SleepState {
    #[inline]
    pub fn is_sleeping(&self) -> bool {
        matches!(self, Self::Asleep)
    }
}

pub use crate::domain::HealthStatus;

/// Strongly-typed call for attention indicator state eliminating boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttentionState {
    #[default]
    Idle,
    Calling,
}

impl AttentionState {
    #[inline]
    pub const fn is_calling(&self) -> bool {
        matches!(self, Self::Calling)
    }
}

/// Ambient look direction for natural gaze idling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LookDirection {
    #[default]
    Center,
    Left,
    Right,
}

/// Stepping leg symmetry for pivot walking. Eliminates primitive obsession.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepLeg {
    #[default]
    Left,
    Right,
}

/// Dance choreography styles driven by Bluetooth music telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MusicDanceStyle {
    #[default]
    ChillSway,
    PopBounce,
    RockHeadbang,
    ChorusFever,
    RhythmNod,
}

/// Comprehensive, strongly-typed pet action vocabulary.
/// All physical, sensor, food, training, and music actions belong to the pet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PetAction {
    // 1. Ambient life
    #[default]
    IdleBreathe,
    LookAround(LookDirection),
    StretchAndYawn,
    ScratchEar,

    // 2. Interaction & Feeding
    HappyHop,
    NoticeFood(FoodType),
    EatingChew {
        food_type: FoodType,
        chew_frame: u8,
    },
    SatisfiedGulp,
    FlushClean {
        tick: u8,
    },
    HealCure {
        tick: u8,
    },

    // 3. Battle Training & Victory
    TrainingHop {
        beat_tick: u8,
    },
    VictoryHop,

    // 4. Music Groove (First-Class Pet Action!)
    MusicDance {
        style: MusicDanceStyle,
        beat_tick: u8,
    },

    // 5. Physical & Sensor Feedback
    TiltBalance {
        angle_deg: i16,
    },
    PivotWalk {
        step_leg: StepLeg,
    },
    SuspendedKick,
    LandCushion,
    ShakeChargeSprint {
        charge_pct: u8,
    },

    // 6. Circadian & Health
    PoopStrain,
    DrowsyNod,
    SleepingZzz {
        bubble_pct: u8,
    },

    // 7. Evolution & Special Moves
    Evolving {
        progress_pct: u8,
        target_species: PetSpecies,
    },
    AttackSpecial {
        tick: u8,
    },
}

/// Action priority hierarchy for deterministic arbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ActionPriority {
    #[default]
    Low = 0, // Ambient life, autonomous wandering, music dancing
    Normal = 1,   // User interactive reactions (HappyHop, Stretch, ShakeChargeSprint)
    High = 2,     // Vital care and combat (NoticeFood, FlushClean, HealCure, AttackSpecial)
    Critical = 3, // Evolving sequence / Sleep override
}

impl PetAction {
    /// Inherent priority of this action.
    #[inline]
    pub const fn default_priority(&self) -> ActionPriority {
        match self {
            Self::Evolving { .. } => ActionPriority::Critical,
            Self::NoticeFood(_)
            | Self::EatingChew { .. }
            | Self::SatisfiedGulp
            | Self::FlushClean { .. }
            | Self::HealCure { .. }
            | Self::AttackSpecial { .. } => ActionPriority::High,
            Self::HappyHop
            | Self::VictoryHop
            | Self::TrainingHop { .. }
            | Self::ShakeChargeSprint { .. }
            | Self::StretchAndYawn
            | Self::ScratchEar
            | Self::LookAround(_) => ActionPriority::Normal,
            Self::IdleBreathe
            | Self::MusicDance { .. }
            | Self::SleepingZzz { .. }
            | Self::PoopStrain
            | Self::DrowsyNod
            | Self::PivotWalk { .. }
            | Self::TiltBalance { .. }
            | Self::SuspendedKick
            | Self::LandCushion => ActionPriority::Low,
        }
    }

    /// Whether this action is uninterruptible by normal user events.
    #[inline]
    pub const fn is_uninterruptible(&self) -> bool {
        matches!(
            self,
            Self::NoticeFood(_)
                | Self::EatingChew { .. }
                | Self::SatisfiedGulp
                | Self::FlushClean { .. }
                | Self::HealCure { .. }
                | Self::AttackSpecial { .. }
                | Self::Evolving { .. }
        )
    }
}

/// Bounded queued action request preserving requested duration and priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedAction {
    pub action: PetAction,
    pub duration_ticks: u16,
    pub priority: ActionPriority,
}

impl QueuedAction {
    #[inline]
    pub const fn new(action: PetAction, duration_ticks: u16) -> Self {
        Self {
            action,
            duration_ticks,
            priority: action.default_priority(),
        }
    }

    #[inline]
    pub const fn with_priority(
        action: PetAction,
        duration_ticks: u16,
        priority: ActionPriority,
    ) -> Self {
        Self {
            action,
            duration_ticks,
            priority,
        }
    }
}
