//! Independent physical and circadian sleep sources with a bounded user override.

use crate::domain::DeviceOrientation;
use embassy_time::{Duration, Instant};

use super::SleepState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CircadianPhase {
    #[default]
    Unknown,
    Day,
    Night,
}

#[derive(Debug, Default)]
pub(super) struct SleepPolicy {
    orientation: DeviceOrientation,
    circadian: CircadianPhase,
    temporary_wake_until: Option<Instant>,
}

impl SleepPolicy {
    pub fn temporary_wake_deadline(&self) -> Option<Instant> {
        self.temporary_wake_until
    }

    pub fn set_orientation(&mut self, orientation: DeviceOrientation) {
        self.orientation = orientation;
        if orientation.is_facedown() {
            self.temporary_wake_until = None;
        }
    }

    pub fn set_hour(&mut self, hour: u8) {
        let phase = if (7..22).contains(&hour) {
            CircadianPhase::Day
        } else {
            CircadianPhase::Night
        };
        if self.circadian != phase {
            self.circadian = phase;
            self.temporary_wake_until = None;
        }
    }

    pub fn clear_hour(&mut self) {
        self.circadian = CircadianPhase::Unknown;
        self.temporary_wake_until = None;
    }

    pub fn request_temporary_wake(&mut self, now: Instant) {
        if self.orientation == DeviceOrientation::FaceUp && self.circadian == CircadianPhase::Night
        {
            self.temporary_wake_until = now.checked_add(Duration::from_secs(30));
        }
    }

    pub fn state(&self, now: Instant) -> SleepState {
        if self.orientation.is_facedown()
            || (self.circadian == CircadianPhase::Night
                && self.temporary_wake_until.is_none_or(|until| now >= until))
        {
            SleepState::Asleep
        } else {
            SleepState::Awake
        }
    }
}
