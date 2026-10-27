//! Monotonic, awake-time physiology, independent of animation frame cadence.

use embassy_time::{Duration, Instant};

use super::{AttentionState, HealthStatus, PetAnimationState, PetMood, PupilShape};
use crate::domain::NurtureProgress;

const DIGESTION_PERIOD: Duration =
    Duration::from_millis(NurtureProgress::DIGESTION_PERIOD_MS as u64);
const STARVATION_LIMIT: Duration =
    Duration::from_millis(NurtureProgress::STARVATION_LIMIT_MS as u64);
const UNCLEAN_LIMIT: Duration = Duration::from_millis(NurtureProgress::UNCLEAN_LIMIT_MS as u64);
const CARE_PERIOD: Duration = Duration::from_millis(NurtureProgress::CARE_PERIOD_MS as u64);
const ZERO: Duration = Duration::from_ticks(0);

#[derive(Debug, Default)]
pub(super) struct NurtureTimers {
    last_update: Option<Instant>,
    digestion: Duration,
    starvation: Duration,
    uncleaned: Duration,
    unattended: Duration,
}

impl NurtureTimers {
    pub fn restore(progress: NurtureProgress, now: Instant) -> Self {
        Self {
            last_update: Some(now),
            digestion: Duration::from_millis(u64::from(progress.digestion_ms())),
            starvation: Duration::from_millis(u64::from(progress.starvation_ms())),
            uncleaned: Duration::from_millis(u64::from(progress.uncleaned_ms())),
            unattended: Duration::from_millis(u64::from(progress.unattended_ms())),
        }
    }

    pub fn snapshot(&self) -> NurtureProgress {
        NurtureProgress::new_clamped(
            self.digestion.as_millis() as u32,
            self.starvation.as_millis() as u32,
            self.uncleaned.as_millis() as u32,
            self.unattended.as_millis() as u32,
        )
    }

    pub fn reset_starvation(&mut self) {
        self.starvation = ZERO;
    }

    pub fn reset_uncleaned(&mut self) {
        self.uncleaned = ZERO;
    }

    pub fn advance_to(&mut self, now: Instant, state: &mut PetAnimationState) {
        let elapsed = match self.last_update {
            Some(previous) => match now.checked_duration_since(previous) {
                Some(elapsed) => elapsed,
                None => return,
            },
            None => ZERO,
        };
        self.last_update = Some(now);
        if state.sleep_state.is_sleeping() {
            return;
        }

        let mut remaining = elapsed;
        // Hunger is capped at four and poop at four: within six digestion events
        // both reach their fixed point. Bound catch-up even after a very long stall.
        for _ in 0..7 {
            if remaining == ZERO {
                break;
            }
            let step = remaining.min(DIGESTION_PERIOD - self.digestion);
            self.advance_conditions(step, state);
            self.digestion += step;
            remaining -= step;
            if self.digestion == DIGESTION_PERIOD {
                self.digestion = ZERO;
                state.hunger = state.hunger.saturating_sub(1);
                if state.hunger <= 1 {
                    state.poop_count = state.poop_count.saturating_add(1).min(4);
                }
                self.advance_conditions(ZERO, state);
            }
        }
        if remaining != ZERO {
            self.advance_conditions(remaining, state);
            advance_periodic(&mut self.digestion, remaining, DIGESTION_PERIOD);
        }
        self.advance_conditions(ZERO, state);
    }

    fn advance_conditions(&mut self, elapsed: Duration, state: &mut PetAnimationState) {
        if state.hunger == 0 {
            add_capped(&mut self.starvation, elapsed, STARVATION_LIMIT);
        } else {
            self.starvation = ZERO;
        }
        if state.poop_count >= 3 {
            add_capped(&mut self.uncleaned, elapsed, UNCLEAN_LIMIT);
        } else {
            self.uncleaned = ZERO;
        }
        if (self.starvation == STARVATION_LIMIT || self.uncleaned == UNCLEAN_LIMIT)
            && !state.is_sick()
        {
            state.health = HealthStatus::Sick;
            state.mood = PetMood::Dizzy;
            state.pupil_shape = PupilShape::Dizzy;
        }

        let needs_attention = state.hunger == 0 || state.poop_count > 0 || state.is_sick();
        state.attention = if needs_attention {
            AttentionState::Calling
        } else {
            AttentionState::Idle
        };
        if needs_attention {
            let missed = advance_periodic(&mut self.unattended, elapsed, CARE_PERIOD);
            state.care_mistakes = state
                .care_mistakes
                .saturating_add(missed.min(u64::from(u8::MAX)) as u8);
        } else {
            self.unattended = ZERO;
        }
    }
}

fn add_capped(total: &mut Duration, elapsed: Duration, limit: Duration) {
    *total = if elapsed >= limit - *total {
        limit
    } else {
        *total + elapsed
    };
}

fn advance_periodic(total: &mut Duration, elapsed: Duration, period: Duration) -> u64 {
    let period_ticks = period.as_ticks();
    // Add only bounded remainders, not two arbitrary elapsed durations.
    let remainder = total.as_ticks() + elapsed.as_ticks() % period_ticks;
    let count = elapsed.as_ticks() / period_ticks + remainder / period_ticks;
    *total = Duration::from_ticks(remainder % period_ticks);
    count
}
