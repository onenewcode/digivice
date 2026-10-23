//! Core domain types and cross-cutting model abstractions.
//!
//! Provides fundamental enums and state representations without tying lower-level layers
//! (storage, gesture) or peripheral drivers to presentation or workflow logic.

pub mod action;
pub mod focus;
pub mod mode;
pub mod music;
pub mod orientation;
pub mod power;

pub use action::HidAction;
pub use focus::{FocusConfig, FocusEvent, FocusPhase};
pub use mode::{InvalidViewMode, ViewMode};
pub use music::{MusicMood, MusicScene, MusicSceneError, MusicSection};
pub use orientation::DeviceOrientation;
pub use power::ChargeState;
