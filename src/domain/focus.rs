//! Domain types for Pomodoro rhythm and focus sessions.

const DEFAULT_FOCUS_SECS: u32 = 25 * 60;
const DEFAULT_SHORT_BREAK_SECS: u32 = 5 * 60;
const DEFAULT_LONG_BREAK_SECS: u32 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusPhase {
    #[default]
    Idle,
    Focus,
    ShortBreak,
    LongBreak,
    Paused,
    Completed,
}

impl FocusPhase {
    #[inline]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Focus | Self::ShortBreak | Self::LongBreak)
    }

    #[inline]
    pub const fn is_rest(self) -> bool {
        matches!(self, Self::ShortBreak | Self::LongBreak)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusEvent {
    Start,
    Pause,
    Resume,
    Skip,
    Reset,
    FaceDown,
    FaceUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusConfig {
    pub focus_secs: u32,
    pub short_break_secs: u32,
    pub long_break_secs: u32,
}

impl Default for FocusConfig {
    fn default() -> Self {
        Self {
            focus_secs: DEFAULT_FOCUS_SECS,
            short_break_secs: DEFAULT_SHORT_BREAK_SECS,
            long_break_secs: DEFAULT_LONG_BREAK_SECS,
        }
    }
}

impl FocusConfig {
    pub const fn new(focus_secs: u32, short_break_secs: u32, long_break_secs: u32) -> Self {
        Self {
            focus_secs,
            short_break_secs,
            long_break_secs,
        }
    }
}
