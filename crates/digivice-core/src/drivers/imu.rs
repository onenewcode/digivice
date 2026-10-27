//! QMI8658 6-axis IMU driver (I2C address 0x6B)

use embassy_time::{Duration, Instant};
use embedded_hal::i2c::I2c;

pub use crate::domain::{ImuError, ImuSample, ImuStatus, ImuTemperature};

pub const QMI8658_ADDR: u8 = 0x6B;

// QMI8658 Register Map
pub const REG_WHO_AM_I: u8 = 0x00;
pub const REG_REVISION_ID: u8 = 0x01;
pub const REG_CTRL1: u8 = 0x02;
pub const REG_CTRL2: u8 = 0x03;
pub const REG_CTRL3: u8 = 0x04;
pub const REG_CTRL5: u8 = 0x06;
pub const REG_CTRL7: u8 = 0x08;
pub const REG_CTRL8: u8 = 0x09;
pub const REG_STATUS0: u8 = 0x2E;
pub const REG_TEMP_L: u8 = 0x33;
pub const REG_AX_L: u8 = 0x35;

pub const WHO_AM_I_VALUE: u8 = 0x05;
pub const IMU_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const FIRST_SAMPLE_TIMEOUT: Duration = Duration::from_secs(1);

/// Strongly-typed IMU sample combined with attitude orientation and filtered gravity.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImuAttitudeSnapshot {
    pub sample: ImuSample,
    pub roll_deg: f32,
    pub pitch_deg: f32,
    pub heading_deg: f32,
    pub gx_filt_mg: i32,
    pub gy_filt_mg: i32,
    pub gz_filt_mg: i32,
    pub imu_status: ImuStatus,
}

impl ImuAttitudeSnapshot {
    pub fn valid_sample(self) -> Option<ImuSample> {
        (self.imu_status == ImuStatus::HardwarePresent).then_some(self.sample)
    }

    pub fn valid_filtered_gravity(self) -> Option<(i32, i32, i32)> {
        (self.imu_status == ImuStatus::HardwarePresent).then_some((
            self.gx_filt_mg,
            self.gy_filt_mg,
            self.gz_filt_mg,
        ))
    }
}

impl core::error::Error for ImuError {}

#[derive(Clone, Copy)]
enum AcquisitionState {
    AwaitingData { deadline: Instant },
    Streaming,
    RetryAt(Instant),
    Stopped,
}

pub struct Qmi8658<I2C> {
    i2c: I2C,
    who_am_i: u8,
    status: ImuStatus,
    acquisition: AcquisitionState,
}

impl<I2C: I2c> Qmi8658<I2C> {
    pub fn new(i2c: I2C) -> Self {
        Self::new_at(i2c, Instant::now())
    }

    pub fn new_at(i2c: I2C, now: Instant) -> Self {
        let mut imu = Self {
            i2c,
            who_am_i: 0,
            status: ImuStatus::default(),
            acquisition: AcquisitionState::Stopped,
        };
        imu.initialize_at(now);
        imu
    }

    fn initialize_at(&mut self, now: Instant) {
        match self.configure_hardware() {
            Ok(()) => {
                if let Some(deadline) = now.checked_add(FIRST_SAMPLE_TIMEOUT) {
                    self.status = ImuStatus::HardwarePresent;
                    self.acquisition = AcquisitionState::AwaitingData { deadline };
                } else {
                    self.status = ImuStatus::Unavailable(ImuError::NotValidated);
                    self.acquisition = AcquisitionState::Stopped;
                }
            }
            Err(error) => {
                self.fail(error, now);
            }
        }
    }

    fn configure_hardware(&mut self) -> Result<(), ImuError> {
        let mut id_buf = [0u8; 1];
        self.who_am_i = 0;
        self.i2c
            .write_read(QMI8658_ADDR, &[REG_WHO_AM_I], &mut id_buf)
            .map_err(|_| ImuError::BusError)?;
        self.who_am_i = id_buf[0];
        if id_buf[0] != WHO_AM_I_VALUE {
            return Err(ImuError::DeviceNotFound);
        }
        // Disable both sensors before restoring configuration after a bus/device fault.
        // Rev A Table 22: code 0x5 is 224.2 Hz in 6DOF, above the 200 Hz host poll rate.
        for register in [
            [REG_CTRL7, 0x00],
            [REG_CTRL1, 0x60],
            [REG_CTRL2, 0x15], // +/-4g, ODR code 0x5.
            [REG_CTRL3, 0x55], // +/-512 dps, the same ODR.
            [REG_CTRL5, 0x55],
            [REG_CTRL7, 0x03],
        ] {
            self.i2c
                .write(QMI8658_ADDR, &register)
                .map_err(|_| ImuError::ConfigurationFailed)?;
        }
        Ok(())
    }

    pub fn status(&self) -> ImuStatus {
        self.status
    }

    pub fn is_available(&self) -> bool {
        self.status == ImuStatus::HardwarePresent
    }

    pub fn who_am_i(&self) -> u8 {
        self.who_am_i
    }

    /// Read only the six inertial axes in one contiguous I2C burst.
    pub fn read_sample(&mut self) -> Result<ImuSample, ImuError> {
        self.read_sample_at(Instant::now())
    }

    pub fn read_sample_at(&mut self, now: Instant) -> Result<ImuSample, ImuError> {
        if let AcquisitionState::RetryAt(at) = self.acquisition
            && now >= at
        {
            self.initialize_at(now);
        }
        if matches!(
            self.acquisition,
            AcquisitionState::RetryAt(_) | AcquisitionState::Stopped
        ) {
            return Err(match self.status {
                ImuStatus::Unavailable(error) => error,
                ImuStatus::HardwarePresent => ImuError::NotValidated,
            });
        }

        match self.acquisition {
            AcquisitionState::AwaitingData { deadline } => {
                if !self.data_ready(now)? {
                    if now >= deadline {
                        self.fail(ImuError::DataNotReady, now);
                    }
                    return Err(ImuError::DataNotReady);
                }
                self.acquisition = AcquisitionState::Streaming;
            }
            AcquisitionState::Streaming => {
                if !self.data_ready(now)? {
                    return Err(ImuError::DataNotReady);
                }
            }
            AcquisitionState::RetryAt(_) | AcquisitionState::Stopped => {
                return Err(ImuError::NotValidated);
            }
        }

        // Read 12 bytes of Accel + Gyro starting at 0x35 (AX_L)
        let mut buf = [0u8; 12];
        self.i2c
            .write_read(QMI8658_ADDR, &[REG_AX_L], &mut buf)
            .map_err(|_| self.fail(ImuError::BusError, now))?;
        self.status = ImuStatus::HardwarePresent;

        let raw_ax = i16::from_le_bytes([buf[0], buf[1]]);
        let raw_ay = i16::from_le_bytes([buf[2], buf[3]]);
        let raw_az = i16::from_le_bytes([buf[4], buf[5]]);

        let raw_gx = i16::from_le_bytes([buf[6], buf[7]]);
        let raw_gy = i16::from_le_bytes([buf[8], buf[9]]);
        let raw_gz = i16::from_le_bytes([buf[10], buf[11]]);

        // Accel ±4g scale: 1g = 8192 LSB -> mg = raw * 1000 / 8192 = raw * 125 / 1024
        // Note: QMI8658 is mounted on the bottom side of the PCB (opposite the GC9A01 LCD).
        // To match the display-centric body coordinate frame (screen facing up = +Z),
        // the Z-axis acceleration and Z gyro rate must be inverted.
        let ax_mg = ((raw_ax as i32) * 125) / 1024;
        let ay_mg = ((raw_ay as i32) * 125) / 1024;
        let az_mg = -(((raw_az as i32) * 125) / 1024);

        // Gyro ±512 dps scale: 1 dps = 64 LSB -> mdps = raw * 1000 / 64 = raw * 125 / 8
        let gx_mdps = ((raw_gx as i32) * 125) / 8;
        let gy_mdps = ((raw_gy as i32) * 125) / 8;
        let gz_mdps = -(((raw_gz as i32) * 125) / 8);

        Ok(ImuSample {
            ax_mg,
            ay_mg,
            az_mg,
            gx_mdps,
            gy_mdps,
            gz_mdps,
        })
    }

    fn data_ready(&mut self, now: Instant) -> Result<bool, ImuError> {
        let mut status = [0];
        self.i2c
            .write_read(QMI8658_ADDR, &[REG_STATUS0], &mut status)
            .map_err(|_| self.fail(ImuError::BusError, now))?;
        Ok(status[0] & 0x03 == 0x03)
    }

    /// Optional low-rate telemetry; failure does not invalidate a six-axis sample.
    pub fn read_temperature(&mut self) -> Result<ImuTemperature, ImuError> {
        if let ImuStatus::Unavailable(error) = self.status {
            return Err(error);
        }
        let mut bytes = [0; 2];
        self.i2c
            .write_read(QMI8658_ADDR, &[REG_TEMP_L], &mut bytes)
            .map_err(|_| ImuError::BusError)?;
        Ok(ImuTemperature::from_raw(i16::from_le_bytes(bytes)))
    }

    fn fail(&mut self, error: ImuError, now: Instant) -> ImuError {
        self.status = ImuStatus::Unavailable(error);
        self.acquisition = match now.checked_add(IMU_RETRY_INTERVAL) {
            Some(at) => AcquisitionState::RetryAt(at),
            None => AcquisitionState::Stopped,
        };
        error
    }

    /// Access inner I2C bus
    pub fn i2c_mut(&mut self) -> &mut I2C {
        &mut self.i2c
    }
}
