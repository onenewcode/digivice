//! Display-centric inertial measurements, independent of the sensor driver.

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ImuSample {
    pub ax_mg: i32,
    pub ay_mg: i32,
    pub az_mg: i32,
    pub gx_mdps: i32,
    pub gy_mdps: i32,
    pub gz_mdps: i32,
}

/// QMI8658 temperature: signed Q8.8 degrees Celsius, independent of inertial data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImuTemperature(i16);

impl ImuTemperature {
    pub const fn from_raw(raw: i16) -> Self {
        Self(raw)
    }

    /// Hundredths of a degree, truncated toward zero; no undocumented offset.
    pub const fn centi_celsius(self) -> i16 {
        (self.0 as i32 * 100 / 256) as i16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImuError {
    DeviceNotFound,
    BusError,
    ConfigurationFailed,
    DataNotReady,
    NotValidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImuStatus {
    HardwarePresent,
    Unavailable(ImuError),
}

impl Default for ImuStatus {
    fn default() -> Self {
        Self::Unavailable(ImuError::NotValidated)
    }
}

impl core::fmt::Display for ImuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DeviceNotFound => write!(f, "QMI8658 device not found or WHO_AM_I mismatch"),
            Self::BusError => write!(f, "I2C bus communication error with QMI8658"),
            Self::ConfigurationFailed => write!(f, "Failed to configure QMI8658 control registers"),
            Self::DataNotReady => write!(f, "QMI8658 inertial data not ready"),
            Self::NotValidated => write!(f, "QMI8658 sample not validated"),
        }
    }
}
