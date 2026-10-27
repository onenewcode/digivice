//! Sensor sampling, attitude filtering, and gesture detection task.

use embassy_time::{Duration, Instant, Ticker, Timer};
use esp_println::println;

use crate::board::power_key::PowerKeyMonitor;
use crate::board::{ButtonDebouncer, ImuAttitudeSnapshot, ImuSample, Qmi8658, SharedI2cDevice};
use crate::gesture::detector::GestureDetector;
use crate::gesture::filter::{ImuFilter, StationaryCalibrator, bounded_dt_seconds};
use crate::ipc::{GESTURE_CHANNEL, IMU_SIGNAL, ORIENTATION_SIGNAL, POWER_SIGNAL, publish_gesture};

/// Task 1: Sensor & Gestures (QMI8658 IMU + Power Rail Monitor)
#[allow(
    clippy::large_stack_frames,
    reason = "Sensor calibration loop and attitude filter state"
)]
#[embassy_executor::task]
pub async fn sensor_task(
    mut imu: Qmi8658<SharedI2cDevice<'static>>,
    mut power_key: PowerKeyMonitor<'static>,
    gyro_bias_mdps: [i32; 3],
    accel_bias_mg: [i16; 3],
) -> ! {
    let mut filter = ImuFilter::new();
    filter.set_accel_bias(accel_bias_mg);
    let mut detector = GestureDetector::new();

    // Initial gyro stationary calibration (accumulate sum directly if IMU is available)
    if imu.is_available() && gyro_bias_mdps == [0; 3] {
        let mut calibration = StationaryCalibrator::new();
        for _ in 0..50 {
            if let Ok(sample) = imu.read_sample() {
                calibration.observe(&sample);
            }
            Timer::after(Duration::from_millis(10)).await;
        }
        match calibration.finish() {
            Ok(bias) => filter.set_gyro_bias(bias.gx_mdps, bias.gy_mdps, bias.gz_mdps),
            Err(error) => println!("WARN: IMU gyro calibration rejected: {:?}", error),
        }
    } else if gyro_bias_mdps != [0; 3] {
        filter.set_gyro_bias(gyro_bias_mdps[0], gyro_bias_mdps[1], gyro_bias_mdps[2]);
        println!("IMU: using persisted gyro bias");
    }

    let mut last_power_sample = Instant::now();
    let mut last_imu_emit = Instant::now();
    let mut last_imu_sample: Option<Instant> = None;
    let mut power_status = power_key.read_status();
    let button_start = Instant::now();
    let mut button_debouncer = ButtonDebouncer::new(power_status.boot_press_state, button_start);
    let mut last_button_sample = button_start;
    let mut ticker = Ticker::every(Duration::from_millis(5));

    loop {
        ticker.next().await;
        let now = Instant::now();

        // 1. Read 6-DOF sample
        if let Ok(sample) = imu.read_sample() {
            let dt_sec = bounded_dt_seconds(last_imu_sample, now);
            last_imu_sample = Some(now);
            let attitude = filter.update(&sample, dt_sec);
            if let Some(event) = detector.process_imu(&sample, &attitude, now)
                && publish_gesture(event, &GESTURE_CHANNEL, &ORIENTATION_SIGNAL).is_err()
            {
                println!("WARN: GESTURE_CHANNEL full; dropping gesture event");
            }

            // Emit attitude update to UI every 30ms (~33Hz smooth tracking)
            if now.duration_since(last_imu_emit) >= Duration::from_millis(30) {
                last_imu_emit = now;
                IMU_SIGNAL.signal(ImuAttitudeSnapshot {
                    sample,
                    roll_deg: attitude.roll_deg,
                    pitch_deg: attitude.pitch_deg,
                    heading_deg: attitude.heading_deg,
                    gx_filt_mg: attitude.gx_filt_mg as i32,
                    gy_filt_mg: attitude.gy_filt_mg as i32,
                    gz_filt_mg: attitude.gz_filt_mg as i32,
                    imu_status: imu.status(),
                });
            }
        } else if now.duration_since(last_imu_emit) >= Duration::from_millis(30) {
            // Broadcast hardware status even if read failed so UI reflects error
            last_imu_emit = now;
            IMU_SIGNAL.signal(ImuAttitudeSnapshot {
                sample: ImuSample::default(),
                roll_deg: 0.0,
                pitch_deg: 0.0,
                heading_deg: 0.0,
                gx_filt_mg: 0,
                gy_filt_mg: 0,
                gz_filt_mg: 1000,
                imu_status: imu.status(),
            });
        }

        // 2. Fast BOOT sampling is independent of the slower ADC/charging telemetry.
        if now.duration_since(last_button_sample) >= Duration::from_millis(20) {
            last_button_sample = now;
            if let Some(state) = button_debouncer.update(power_key.button_state(), now) {
                power_status.boot_press_state = state;
                POWER_SIGNAL.signal(power_status);
            }
        }

        // 3. Periodic Power Rail sampling (every 200ms)
        if now.duration_since(last_power_sample) >= Duration::from_millis(200) {
            last_power_sample = now;
            let boot_state = button_debouncer.state();
            power_status = power_key.read_status();
            power_status.boot_press_state = boot_state;
            POWER_SIGNAL.signal(power_status);
        }
    }
}
