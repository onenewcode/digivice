//! Domain models for music telemetry, rhythm features, and scene sync.

use embassy_time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MusicMood {
    #[default]
    Unknown,
    Calm,
    Happy,
    Focused,
    Excited,
    Sad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MusicSection {
    #[default]
    Unknown,
    Intro,
    Verse,
    Chorus,
    Bridge,
    Outro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MusicActivity {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicSceneError {
    FrameTooShort,
    UnsupportedVersion,
    InvalidBpm,
}

impl core::fmt::Display for MusicSceneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FrameTooShort => f.write_str("MusicScene frame is shorter than 10 bytes"),
            Self::UnsupportedVersion => f.write_str("MusicScene frame version is unsupported"),
            Self::InvalidBpm => f.write_str("MusicScene BPM must be zero or within 40..=240"),
        }
    }
}

impl core::error::Error for MusicSceneError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicScene {
    pub sequence: u16,
    pub bpm: u16,
    pub beat_phase: u8,
    pub energy: u8,
    pub mood: MusicMood,
    pub section: MusicSection,
    pub received_at: Instant,
    activity: MusicActivity,
}

impl MusicScene {
    pub const INACTIVE_AFTER: Duration = Duration::from_secs(3);

    pub fn new(
        sequence: u16,
        bpm: u16,
        beat_phase: u8,
        energy: u8,
        mood: MusicMood,
        section: MusicSection,
        now: Instant,
    ) -> Self {
        Self {
            sequence,
            bpm: bpm.clamp(0, 240),
            beat_phase,
            energy: energy.min(100),
            mood,
            section,
            received_at: now,
            activity: MusicActivity::Active,
        }
    }

    pub const fn inactive(now: Instant) -> Self {
        Self {
            sequence: 0,
            bpm: 0,
            beat_phase: 0,
            energy: 0,
            mood: MusicMood::Unknown,
            section: MusicSection::Unknown,
            received_at: now,
            activity: MusicActivity::Inactive,
        }
    }

    /// Decode the compact GATT MusicScene payload.
    ///
    /// Layout: version(1), sequence(2 LE), bpm(2 LE), beat phase(1),
    /// energy(1), mood(1), section(1), flags(1).
    pub fn from_frame(frame: &[u8], now: Instant) -> Result<Self, MusicSceneError> {
        if frame.len() < 10 {
            return Err(MusicSceneError::FrameTooShort);
        }
        if frame[0] != 1 {
            return Err(MusicSceneError::UnsupportedVersion);
        }
        let sequence = u16::from_le_bytes([frame[1], frame[2]]);
        let bpm = u16::from_le_bytes([frame[3], frame[4]]);
        if bpm != 0 && !(40..=240).contains(&bpm) {
            return Err(MusicSceneError::InvalidBpm);
        }
        let mood = match frame[7] {
            1 => MusicMood::Calm,
            2 => MusicMood::Happy,
            3 => MusicMood::Focused,
            4 => MusicMood::Excited,
            5 => MusicMood::Sad,
            _ => MusicMood::Unknown,
        };
        let section = match frame[8] {
            1 => MusicSection::Intro,
            2 => MusicSection::Verse,
            3 => MusicSection::Chorus,
            4 => MusicSection::Bridge,
            5 => MusicSection::Outro,
            _ => MusicSection::Unknown,
        };
        Ok(Self::new(
            sequence, bpm, frame[5], frame[6], mood, section, now,
        ))
    }

    #[inline]
    pub fn is_active(&self, now: Instant) -> bool {
        matches!(self.activity, MusicActivity::Active)
            && now.duration_since(self.received_at) < Self::INACTIVE_AFTER
    }

    /// Accept only newer sequence numbers, including across u16 wraparound.
    pub fn is_newer_than(&self, previous: &Self) -> bool {
        self.sequence != previous.sequence && self.sequence.wrapping_sub(previous.sequence) < 0x8000
    }
}
