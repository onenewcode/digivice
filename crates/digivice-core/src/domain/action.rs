//! Domain-level user interaction actions and media control intents.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidAction {
    PlayPause,
    PreviousTrack,
    NextTrack,
    VolumeUp,
    VolumeDown,
    Mute,
}
