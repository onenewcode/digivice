//! IMU filtering, zero-bias calibration, and attitude solving

use crate::domain::ImuSample;
use embassy_time::{Duration, Instant};
use libm::fabsf;

pub const MAX_FILTER_INTERVAL: Duration = Duration::from_millis(100);
pub const MIN_CALIBRATION_SAMPLES: u16 = 32;
const MAX_STATIONARY_GYRO_MDPS: i64 = 5_000;
const MIN_GRAVITY_SQ_MG: i64 = 850 * 850;
const MAX_GRAVITY_SQ_MG: i64 = 1_150 * 1_150;
const MAX_GYRO_SPREAD_MDPS: i32 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GyroBias {
    pub gx_mdps: i32,
    pub gy_mdps: i32,
    pub gz_mdps: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationError {
    TooFewSamples,
    MovingDevice,
    InvalidGravity,
}

/// Bounded online calibration window; samples are never copied onto the task stack.
#[derive(Debug, Clone, Copy)]
pub struct StationaryCalibrator {
    count: u16,
    sum_gx: i64,
    sum_gy: i64,
    sum_gz: i64,
    min_gx: i32,
    min_gy: i32,
    min_gz: i32,
    max_gx: i32,
    max_gy: i32,
    max_gz: i32,
    invalid_gravity: bool,
    moving: bool,
}

impl StationaryCalibrator {
    pub const fn new() -> Self {
        Self {
            count: 0,
            sum_gx: 0,
            sum_gy: 0,
            sum_gz: 0,
            min_gx: i32::MAX,
            min_gy: i32::MAX,
            min_gz: i32::MAX,
            max_gx: i32::MIN,
            max_gy: i32::MIN,
            max_gz: i32::MIN,
            invalid_gravity: false,
            moving: false,
        }
    }

    pub fn observe(&mut self, sample: &ImuSample) {
        let ax = i64::from(sample.ax_mg);
        let ay = i64::from(sample.ay_mg);
        let az = i64::from(sample.az_mg);
        let gravity_sq = ax * ax + ay * ay + az * az;
        if !(MIN_GRAVITY_SQ_MG..=MAX_GRAVITY_SQ_MG).contains(&gravity_sq) {
            self.invalid_gravity = true;
        }
        let gx = sample.gx_mdps;
        let gy = sample.gy_mdps;
        let gz = sample.gz_mdps;
        if i64::from(gx).abs() > MAX_STATIONARY_GYRO_MDPS
            || i64::from(gy).abs() > MAX_STATIONARY_GYRO_MDPS
            || i64::from(gz).abs() > MAX_STATIONARY_GYRO_MDPS
        {
            self.moving = true;
        }
        self.count = self.count.saturating_add(1);
        self.sum_gx += i64::from(gx);
        self.sum_gy += i64::from(gy);
        self.sum_gz += i64::from(gz);
        self.min_gx = self.min_gx.min(gx);
        self.min_gy = self.min_gy.min(gy);
        self.min_gz = self.min_gz.min(gz);
        self.max_gx = self.max_gx.max(gx);
        self.max_gy = self.max_gy.max(gy);
        self.max_gz = self.max_gz.max(gz);
    }

    pub fn finish(self) -> Result<GyroBias, CalibrationError> {
        if self.count < MIN_CALIBRATION_SAMPLES {
            return Err(CalibrationError::TooFewSamples);
        }
        if self.invalid_gravity {
            return Err(CalibrationError::InvalidGravity);
        }
        if self.moving
            || self.max_gx - self.min_gx > MAX_GYRO_SPREAD_MDPS
            || self.max_gy - self.min_gy > MAX_GYRO_SPREAD_MDPS
            || self.max_gz - self.min_gz > MAX_GYRO_SPREAD_MDPS
        {
            return Err(CalibrationError::MovingDevice);
        }
        let count = i64::from(self.count);
        Ok(GyroBias {
            gx_mdps: (self.sum_gx / count) as i32,
            gy_mdps: (self.sum_gy / count) as i32,
            gz_mdps: (self.sum_gz / count) as i32,
        })
    }
}

impl Default for StationaryCalibrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert an observed sample interval into seconds without integrating across stalls.
pub fn bounded_dt_seconds(previous: Option<Instant>, now: Instant) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };
    let Some(elapsed) = now.checked_duration_since(previous) else {
        return 0.0;
    };
    if elapsed.as_micros() == 0 || elapsed > MAX_FILTER_INTERVAL {
        0.0
    } else {
        elapsed.as_micros() as f32 / 1_000_000.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalibrationState {
    Uncalibrated,
    Calibrated,
}

/// Zero-FPU fast rational arctangent approximation in degrees.
/// Typical absolute error is below 0.3 degrees across the 360 degree space.
/// Replaces heavy `libm::atan2f` software emulation routines on RISC-V RV32IMAC cores.
#[inline]
pub fn fast_atan2_deg(y: f32, x: f32) -> f32 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    let ax = fabsf(x);
    let ay = fabsf(y);
    let (octant_swap, r) = if ax > ay {
        (false, ay / ax)
    } else {
        (true, ax / ay)
    };
    // Hastings rational approximation: atan(r) in degrees on [0, 1]
    let angle_sub = (r * 57.295_78) / (1.0 + 0.280_86 * r * r);
    let angle = if octant_swap {
        90.0 - angle_sub
    } else {
        angle_sub
    };
    let signed_angle = if x >= 0.0 { angle } else { 180.0 - angle };
    if y < 0.0 { -signed_angle } else { signed_angle }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Attitude {
    pub roll_deg: f32,
    pub pitch_deg: f32,
    pub heading_deg: f32,
    pub gx_filt_mg: f32,
    pub gy_filt_mg: f32,
    pub gz_filt_mg: f32,
    pub gz_dps: f32,
}

pub struct ImuFilter {
    // Gyro zero bias in mdps
    pub gx_bias_mdps: i32,
    pub gy_bias_mdps: i32,
    pub gz_bias_mdps: i32,
    accel_bias_mg: [i16; 3],
    // Filtered gravity vector
    gx_lpf: f32,
    gy_lpf: f32,
    gz_lpf: f32,
    // Heading in degrees [0..360)
    pub heading: f32,
    // Alpha for 1st order LPF (~0.15 for smooth gravity vector at 250Hz)
    alpha: f32,
    calibration: CalibrationState,
}

impl Default for ImuFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ImuFilter {
    pub fn new() -> Self {
        Self {
            gx_bias_mdps: 0,
            gy_bias_mdps: 0,
            gz_bias_mdps: 0,
            accel_bias_mg: [0; 3],
            gx_lpf: 0.0,
            gy_lpf: 0.0,
            gz_lpf: 1000.0,
            heading: 0.0,
            alpha: 0.28,
            calibration: CalibrationState::Uncalibrated,
        }
    }

    /// Set manual or loaded calibration bias
    pub fn set_gyro_bias(&mut self, gx: i32, gy: i32, gz: i32) {
        self.gx_bias_mdps = gx;
        self.gy_bias_mdps = gy;
        self.gz_bias_mdps = gz;
        self.calibration = CalibrationState::Calibrated;
    }

    pub fn set_accel_bias(&mut self, bias: [i16; 3]) {
        self.accel_bias_mg = bias;
    }

    /// Calibrate from a collection of samples recorded while the board is stationary on desk
    pub fn calibrate_from_samples(&mut self, samples: &[ImuSample]) {
        if samples.is_empty() {
            return;
        }
        let mut sum_gx: i64 = 0;
        let mut sum_gy: i64 = 0;
        let mut sum_gz: i64 = 0;
        for s in samples {
            sum_gx += s.gx_mdps as i64;
            sum_gy += s.gy_mdps as i64;
            sum_gz += s.gz_mdps as i64;
        }
        let count = samples.len() as i64;
        self.gx_bias_mdps = (sum_gx / count) as i32;
        self.gy_bias_mdps = (sum_gy / count) as i32;
        self.gz_bias_mdps = (sum_gz / count) as i32;
        self.calibration = CalibrationState::Calibrated;
    }

    /// Update filter with a new sample at interval dt_sec (e.g. 0.004s for 250Hz)
    pub fn update(&mut self, sample: &ImuSample, dt_sec: f32) -> Attitude {
        // Subtract gyro bias
        let gz_corr_mdps = sample.gz_mdps - self.gz_bias_mdps;
        let gz_dps = (gz_corr_mdps as f32) / 1000.0;

        // Low-pass filter for gravity vector
        let ax = (sample.ax_mg - i32::from(self.accel_bias_mg[0])) as f32;
        let ay = (sample.ay_mg - i32::from(self.accel_bias_mg[1])) as f32;
        let az = (sample.az_mg - i32::from(self.accel_bias_mg[2])) as f32;

        self.gx_lpf = self.gx_lpf + self.alpha * (ax - self.gx_lpf);
        self.gy_lpf = self.gy_lpf + self.alpha * (ay - self.gy_lpf);
        self.gz_lpf = self.gz_lpf + self.alpha * (az - self.gz_lpf);

        // Calculate Roll and Pitch using fast rational approximation
        let roll_deg = fast_atan2_deg(self.gx_lpf, self.gz_lpf);
        let pitch_deg = fast_atan2_deg(self.gy_lpf, self.gz_lpf);

        // Heading integration: only integrate when board is flat on table (|gz| > 750mg)
        // and rotation speed is above deadzone (|gz_dps| > 2.0 dps) to eliminate drift
        if self.gz_lpf > 750.0 && fabsf(gz_dps) > 2.0 {
            self.heading -= gz_dps * dt_sec;
            if self.heading >= 360.0 {
                self.heading -= 360.0;
            } else if self.heading < 0.0 {
                self.heading += 360.0;
            }
        }

        Attitude {
            roll_deg,
            pitch_deg,
            heading_deg: self.heading,
            gx_filt_mg: self.gx_lpf,
            gy_filt_mg: self.gy_lpf,
            gz_filt_mg: self.gz_lpf,
            gz_dps,
        }
    }
}
