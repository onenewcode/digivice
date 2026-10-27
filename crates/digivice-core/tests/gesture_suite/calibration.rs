use digivice_core::domain::ImuSample;
use digivice_core::gesture::filter::{
    CalibrationError, MIN_CALIBRATION_SAMPLES, StationaryCalibrator,
};

fn sample(gx_mdps: i32, gy_mdps: i32, gz_mdps: i32, az_mg: i32) -> ImuSample {
    ImuSample {
        ax_mg: 0,
        ay_mg: 0,
        az_mg,
        gx_mdps,
        gy_mdps,
        gz_mdps,
    }
}

#[test]
fn a_stable_gravity_window_accepts_bias_without_copying_samples() {
    let mut window = StationaryCalibrator::new();
    for _ in 0..MIN_CALIBRATION_SAMPLES {
        window.observe(&sample(120, -80, 40, 1000));
    }
    assert_eq!(
        window.finish(),
        Ok(digivice_core::gesture::filter::GyroBias {
            gx_mdps: 120,
            gy_mdps: -80,
            gz_mdps: 40,
        })
    );
}

#[test]
fn too_few_samples_never_calibrate_even_if_each_sample_is_stable() {
    let mut window = StationaryCalibrator::new();
    for _ in 0..MIN_CALIBRATION_SAMPLES - 1 {
        window.observe(&sample(0, 0, 0, 1000));
    }
    assert_eq!(window.finish(), Err(CalibrationError::TooFewSamples));
}

#[test]
fn motion_and_bad_gravity_are_rejected_instead_of_becoming_a_zero_bias() {
    let mut moving = StationaryCalibrator::new();
    for index in 0..MIN_CALIBRATION_SAMPLES {
        moving.observe(&sample(if index == 10 { 6_000 } else { 0 }, 0, 0, 1000));
    }
    assert_eq!(moving.finish(), Err(CalibrationError::MovingDevice));

    let mut bad_gravity = StationaryCalibrator::new();
    for _ in 0..MIN_CALIBRATION_SAMPLES {
        bad_gravity.observe(&sample(0, 0, 0, 1_500));
    }
    assert_eq!(bad_gravity.finish(), Err(CalibrationError::InvalidGravity));
}

#[test]
fn excessive_stationary_jitter_is_rejected_even_when_each_peak_is_below_motion_limit() {
    let mut window = StationaryCalibrator::new();
    for index in 0..MIN_CALIBRATION_SAMPLES {
        window.observe(&sample(
            if index % 2 == 0 { -1_100 } else { 1_100 },
            0,
            0,
            1000,
        ));
    }
    assert_eq!(window.finish(), Err(CalibrationError::MovingDevice));
}
