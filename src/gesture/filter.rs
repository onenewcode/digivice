//! IMU filtering, zero-bias calibration, and attitude solving

use crate::board::ImuSample;
use core::f32::consts::PI;
use libm::{atan2f, fabsf};

const RAD_TO_DEG: f32 = 180.0 / PI;

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
    // Filtered gravity vector
    gx_lpf: f32,
    gy_lpf: f32,
    gz_lpf: f32,
    // Heading in degrees [0..360)
    pub heading: f32,
    // Alpha for 1st order LPF (~0.15 for smooth gravity vector at 250Hz)
    alpha: f32,
    calibrated: bool,
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
            gx_lpf: 0.0,
            gy_lpf: 0.0,
            gz_lpf: 1000.0,
            heading: 0.0,
            alpha: 0.15,
            calibrated: false,
        }
    }

    /// Set manual or loaded calibration bias
    pub fn set_gyro_bias(&mut self, gx: i32, gy: i32, gz: i32) {
        self.gx_bias_mdps = gx;
        self.gy_bias_mdps = gy;
        self.gz_bias_mdps = gz;
        self.calibrated = true;
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
        self.calibrated = true;
    }

    /// Update filter with a new sample at interval dt_sec (e.g. 0.004s for 250Hz)
    pub fn update(&mut self, sample: &ImuSample, dt_sec: f32) -> Attitude {
        // Subtract gyro bias
        let gz_corr_mdps = sample.gz_mdps - self.gz_bias_mdps;
        let gz_dps = (gz_corr_mdps as f32) / 1000.0;

        // Low-pass filter for gravity vector
        let ax = sample.ax_mg as f32;
        let ay = sample.ay_mg as f32;
        let az = sample.az_mg as f32;

        self.gx_lpf = self.gx_lpf + self.alpha * (ax - self.gx_lpf);
        self.gy_lpf = self.gy_lpf + self.alpha * (ay - self.gy_lpf);
        self.gz_lpf = self.gz_lpf + self.alpha * (az - self.gz_lpf);

        // Calculate Roll and Pitch using atan2f
        let roll_deg = atan2f(self.gx_lpf, self.gz_lpf) * RAD_TO_DEG;
        let pitch_deg = atan2f(self.gy_lpf, self.gz_lpf) * RAD_TO_DEG;

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
