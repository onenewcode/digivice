//! Generic chip sensor and peripheral drivers (independent of MCU).

pub mod imu;
pub mod rtc;

pub use imu::{ImuAttitudeSnapshot, ImuSample, ImuStatus, ImuTemperature, Qmi8658};
pub use rtc::{DateTime, Pcf85063, RtcError, RtcStatus};
