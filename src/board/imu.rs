//! QMI8658 6-axis IMU driver (I2C address 0x6B)

use embedded_hal::i2c::I2c;

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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ImuSample {
    pub ax_mg: i32,
    pub ay_mg: i32,
    pub az_mg: i32,
    pub gx_mdps: i32,
    pub gy_mdps: i32,
    pub gz_mdps: i32,
    pub temp_c: i16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImuError {
    DeviceNotFound,
    BusError,
    ConfigurationFailed,
}

pub struct Qmi8658<I2C> {
    i2c: I2C,
    who_am_i: u8,
}

impl<I2C: I2c> Qmi8658<I2C> {
    pub fn new(mut i2c: I2C) -> Result<Self, ImuError> {
        let mut id_buf = [0u8; 1];
        if i2c
            .write_read(QMI8658_ADDR, &[REG_WHO_AM_I], &mut id_buf)
            .is_err()
        {
            return Err(ImuError::BusError);
        }

        if id_buf[0] != WHO_AM_I_VALUE {
            return Err(ImuError::DeviceNotFound);
        }

        let who_am_i = id_buf[0];

        // 1. CTRL1: 0x60 (auto-increment register address, 50MHz serial, internal 2MHz clock)
        i2c.write(QMI8658_ADDR, &[REG_CTRL1, 0x60])
            .map_err(|_| ImuError::ConfigurationFailed)?;

        // 2. CTRL2: Accel: ±4g (0x10), ODR 250Hz (0x07) -> 0x17
        i2c.write(QMI8658_ADDR, &[REG_CTRL2, 0x17])
            .map_err(|_| ImuError::ConfigurationFailed)?;

        // 3. CTRL3: Gyro: ±512 dps (0x50), ODR 250Hz (0x07) -> 0x57
        i2c.write(QMI8658_ADDR, &[REG_CTRL3, 0x57])
            .map_err(|_| ImuError::ConfigurationFailed)?;

        // 4. CTRL5: Enable low-pass filter (LPF Mode 0, ~10% ODR cutoff)
        i2c.write(QMI8658_ADDR, &[REG_CTRL5, 0x55])
            .map_err(|_| ImuError::ConfigurationFailed)?;

        // 5. CTRL7: Enable accelerometer and gyroscope
        i2c.write(QMI8658_ADDR, &[REG_CTRL7, 0x03])
            .map_err(|_| ImuError::ConfigurationFailed)?;

        Ok(Self { i2c, who_am_i })
    }

    pub fn who_am_i(&self) -> u8 {
        self.who_am_i
    }

    /// Read raw 6-DOF + Temperature in a single contiguous I2C burst read
    pub fn read_sample(&mut self) -> Result<ImuSample, ImuError> {
        // Read 12 bytes of Accel + Gyro starting at 0x35 (AX_L)
        let mut buf = [0u8; 12];
        self.i2c
            .write_read(QMI8658_ADDR, &[REG_AX_L], &mut buf)
            .map_err(|_| ImuError::BusError)?;

        let raw_ax = i16::from_le_bytes([buf[0], buf[1]]);
        let raw_ay = i16::from_le_bytes([buf[2], buf[3]]);
        let raw_az = i16::from_le_bytes([buf[4], buf[5]]);

        let raw_gx = i16::from_le_bytes([buf[6], buf[7]]);
        let raw_gy = i16::from_le_bytes([buf[8], buf[9]]);
        let raw_gz = i16::from_le_bytes([buf[10], buf[11]]);

        // Accel ±4g scale: 1g = 8192 LSB -> mg = raw * 1000 / 8192 = raw * 125 / 1024
        let ax_mg = ((raw_ax as i32) * 125) / 1024;
        let ay_mg = ((raw_ay as i32) * 125) / 1024;
        let az_mg = ((raw_az as i32) * 125) / 1024;

        // Gyro ±512 dps scale: 1 dps = 64 LSB -> mdps = raw * 1000 / 64 = raw * 125 / 8
        let gx_mdps = ((raw_gx as i32) * 125) / 8;
        let gy_mdps = ((raw_gy as i32) * 125) / 8;
        let gz_mdps = ((raw_gz as i32) * 125) / 8;

        // Read temperature (0x33, 0x34)
        let mut t_buf = [0u8; 2];
        let _ = self.i2c.write_read(QMI8658_ADDR, &[REG_TEMP_L], &mut t_buf);
        let raw_temp = i16::from_le_bytes([t_buf[0], t_buf[1]]);
        // Temperature formula from datasheet: temp_c = raw_temp / 256.0 + 25.0
        let temp_c = (raw_temp / 256) + 25;

        Ok(ImuSample {
            ax_mg,
            ay_mg,
            az_mg,
            gx_mdps,
            gy_mdps,
            gz_mdps,
            temp_c,
        })
    }

    /// Access inner I2C bus
    pub fn i2c_mut(&mut self) -> &mut I2C {
        &mut self.i2c
    }
}
