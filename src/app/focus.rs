//! Local focus rhythm and Pomodoro session workflow engine.
//!
//! The timer is intentionally independent from USB/BLE connectivity so the device
//! maintains its local productivity rhythm even when no host or companion app is present.

use embassy_time::{Duration, Instant};

use crate::domain::{FocusConfig, FocusEvent, FocusPhase};

const LONG_BREAK_EVERY: u8 = 4;

const fn clamp_u32(value: u32, min: u32, max: u32) -> u32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

pub const fn clamp_focus_config(config: FocusConfig) -> FocusConfig {
    FocusConfig {
        focus_secs: clamp_u32(config.focus_secs, 60, 2 * 60 * 60),
        short_break_secs: clamp_u32(config.short_break_secs, 30, 60 * 60),
        long_break_secs: clamp_u32(config.long_break_secs, 60, 2 * 60 * 60),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PauseReason {
    Manual,
    FaceDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResumeSource {
    Manual,
    FaceUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusSession {
    config: FocusConfig,
    phase: FocusPhase,
    deadline: Option<Instant>,
    completed_cycles: u8,
    paused_phase: Option<FocusPhase>,
    paused_remaining_secs: u32,
    pause_reason: Option<PauseReason>,
}

impl Default for FocusSession {
    fn default() -> Self {
        Self::new(FocusConfig::default())
    }
}

impl FocusSession {
    pub const fn new(config: FocusConfig) -> Self {
        Self {
            config: clamp_focus_config(config),
            phase: FocusPhase::Idle,
            deadline: None,
            completed_cycles: 0,
            paused_phase: None,
            paused_remaining_secs: 0,
            pause_reason: None,
        }
    }

    #[inline]
    pub const fn phase(&self) -> FocusPhase {
        self.phase
    }

    #[inline]
    pub const fn completed_cycles(&self) -> u8 {
        self.completed_cycles
    }

    pub const fn config(&self) -> FocusConfig {
        self.config
    }

    pub fn apply(&mut self, event: FocusEvent, now: Instant) {
        match event {
            FocusEvent::Start => self.start_focus(now),
            FocusEvent::Pause => {
                if self.phase.is_active() {
                    self.pause(now, PauseReason::Manual);
                }
            }
            FocusEvent::FaceDown => {
                if self.phase.is_active() {
                    self.pause(now, PauseReason::FaceDown);
                }
            }
            FocusEvent::Resume => self.resume(now, ResumeSource::Manual),
            FocusEvent::FaceUp => self.resume(now, ResumeSource::FaceUp),
            FocusEvent::Skip => self.advance(now),
            FocusEvent::Reset => {
                self.phase = FocusPhase::Idle;
                self.deadline = None;
                self.completed_cycles = 0;
                self.clear_pause();
            }
        }
    }

    pub fn toggle(&mut self, now: Instant) {
        match self.phase {
            FocusPhase::Idle | FocusPhase::Completed => self.apply(FocusEvent::Start, now),
            FocusPhase::Paused => self.apply(FocusEvent::Resume, now),
            FocusPhase::Focus | FocusPhase::ShortBreak | FocusPhase::LongBreak => {
                self.apply(FocusEvent::Pause, now)
            }
        }
    }

    pub fn tick(&mut self, now: Instant) -> FocusPhase {
        let Some(deadline) = self.deadline else {
            return self.phase;
        };
        if now < deadline {
            return self.phase;
        }
        self.advance(now);
        self.phase
    }

    fn start_focus(&mut self, now: Instant) {
        self.phase = FocusPhase::Focus;
        self.deadline = Some(now + Duration::from_secs(self.config.focus_secs as u64));
        self.clear_pause();
    }

    fn pause(&mut self, now: Instant, reason: PauseReason) {
        self.paused_phase = Some(self.phase);
        self.paused_remaining_secs = self
            .deadline
            // A gesture can race the maintenance tick after the deadline. Saturating here
            // preserves the pause transition instead of panicking on an expired deadline.
            .map(|deadline| deadline.saturating_duration_since(now).as_secs().max(1) as u32)
            .unwrap_or(1);
        self.pause_reason = Some(reason);
        self.phase = FocusPhase::Paused;
        self.deadline = None;
    }

    fn resume(&mut self, now: Instant, source: ResumeSource) {
        if self.phase != FocusPhase::Paused
            || (source == ResumeSource::FaceUp && self.pause_reason != Some(PauseReason::FaceDown))
        {
            return;
        }
        let phase = self.paused_phase.unwrap_or(FocusPhase::Focus);
        self.phase = phase;
        self.deadline = Some(now + Duration::from_secs(self.paused_remaining_secs as u64));
        self.clear_pause();
    }

    fn clear_pause(&mut self) {
        self.paused_phase = None;
        self.paused_remaining_secs = 0;
        self.pause_reason = None;
    }

    fn advance(&mut self, now: Instant) {
        match self.phase {
            FocusPhase::Focus => {
                self.completed_cycles = self.completed_cycles.saturating_add(1);
                self.phase = if self.completed_cycles.is_multiple_of(LONG_BREAK_EVERY) {
                    FocusPhase::LongBreak
                } else {
                    FocusPhase::ShortBreak
                };
                self.deadline = Some(
                    now + Duration::from_secs(if self.phase == FocusPhase::LongBreak {
                        self.config.long_break_secs
                    } else {
                        self.config.short_break_secs
                    } as u64),
                );
                self.clear_pause();
            }
            FocusPhase::ShortBreak | FocusPhase::LongBreak => self.start_focus(now),
            FocusPhase::Paused | FocusPhase::Idle | FocusPhase::Completed => {}
        }
    }
}
