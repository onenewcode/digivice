//! Durable nurture values, separate from animation and boot-local monotonic timestamps.

use super::CalendarDay;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum HealthStatus {
    #[default]
    Healthy = 0,
    Sick = 1,
}

impl HealthStatus {
    pub const fn is_sick(&self) -> bool {
        matches!(self, Self::Sick)
    }
}

impl TryFrom<u8> for HealthStatus {
    type Error = InvalidHealthStatus;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Healthy),
            1 => Ok(Self::Sick),
            _ => Err(InvalidHealthStatus),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidHealthStatus;

/// Accumulated awake milliseconds, never a stale Instant from a previous boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NurtureProgress {
    digestion_ms: u32,
    starvation_ms: u32,
    uncleaned_ms: u32,
    unattended_ms: u32,
}

impl NurtureProgress {
    pub const DIGESTION_PERIOD_MS: u32 = 30_000;
    pub const STARVATION_LIMIT_MS: u32 = 60_000;
    pub const UNCLEAN_LIMIT_MS: u32 = 40_000;
    pub const CARE_PERIOD_MS: u32 = 180_000;

    pub const fn new_clamped(
        digestion_ms: u32,
        starvation_ms: u32,
        uncleaned_ms: u32,
        unattended_ms: u32,
    ) -> Self {
        Self {
            digestion_ms: clamp_max(digestion_ms, Self::DIGESTION_PERIOD_MS - 1),
            starvation_ms: clamp_max(starvation_ms, Self::STARVATION_LIMIT_MS),
            uncleaned_ms: clamp_max(uncleaned_ms, Self::UNCLEAN_LIMIT_MS),
            unattended_ms: clamp_max(unattended_ms, Self::CARE_PERIOD_MS - 1),
        }
    }

    pub const fn digestion_ms(self) -> u32 {
        self.digestion_ms
    }

    pub const fn starvation_ms(self) -> u32 {
        self.starvation_ms
    }

    pub const fn uncleaned_ms(self) -> u32 {
        self.uncleaned_ms
    }

    pub const fn unattended_ms(self) -> u32 {
        self.unattended_ms
    }
}

const fn clamp_max(value: u32, max: u32) -> u32 {
    if value > max { max } else { value }
}

/// Transfer fields. Apply them only through the validated `PetArchive` constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetArchiveData {
    pub stage: u8,
    pub hunger: u8,
    pub strength: u8,
    pub weight_g: u8,
    pub care_mistakes: u8,
    pub training_count: u16,
    pub age_days: u16,
    pub health: HealthStatus,
    pub poop_count: u8,
    pub energy: u8,
    pub affinity: u16,
    pub progress: NurtureProgress,
    pub calendar_day: Option<CalendarDay>,
}

impl Default for PetArchiveData {
    fn default() -> Self {
        Self {
            stage: 0,
            hunger: 4,
            strength: 4,
            weight_g: 5,
            care_mistakes: 0,
            training_count: 0,
            age_days: 1,
            health: HealthStatus::Healthy,
            poop_count: 0,
            energy: 100,
            affinity: 0,
            progress: NurtureProgress::default(),
            calendar_day: None,
        }
    }
}

/// Canonical durable state; contains no view, animation, session or physical sensor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PetArchive {
    data: PetArchiveData,
}

impl PetArchive {
    pub fn new_clamped(mut data: PetArchiveData) -> Self {
        data.stage = data.stage.min(5);
        data.hunger = data.hunger.min(4);
        data.strength = data.strength.min(4);
        data.weight_g = data.weight_g.max(1);
        data.age_days = data.age_days.max(1);
        data.poop_count = data.poop_count.min(4);
        data.energy = data.energy.min(100);
        Self { data }
    }

    pub const fn data(self) -> PetArchiveData {
        self.data
    }
}
