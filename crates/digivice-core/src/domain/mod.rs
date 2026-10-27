//! Core domain types and cross-cutting model abstractions.
//!
//! Provides fundamental enums and state representations without tying lower-level layers
//! (storage, gesture) or peripheral drivers to presentation or workflow logic.

pub mod action;
pub mod clock;
pub mod imu;
pub mod mode;
pub mod music;
pub mod nurture;
pub mod orientation;
pub mod power;

pub use action::HidAction;
pub use clock::{
    CalendarDay, CircadianTime, ClockSnapshot, DateTime, DateTimeError, RtcError, RtcStatus,
};
pub use imu::{ImuError, ImuSample, ImuStatus, ImuTemperature};
pub use mode::{InvalidViewMode, ViewMode};
pub use music::{MusicMood, MusicScene, MusicSceneError, MusicSection};
pub use nurture::{HealthStatus, NurtureProgress, PetArchive, PetArchiveData};
pub use orientation::DeviceOrientation;
pub use power::{
    ButtonDebouncer, ButtonPressState, ChargeState, VoltageMeasurement, VoltageValidity,
};
