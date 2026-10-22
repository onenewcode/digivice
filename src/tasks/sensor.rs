//! Sensor sampling, attitude filtering, and gesture detection task.

use embassy_time::{Duration, Instant, Ticker, Timer};
use esp_println::println;

use crate::board::power_key::PowerKeyMonitor;
use crate::board::{ImuAttitudeSnapshot, ImuSample, Qmi8658, SharedI2cDevice};
use crate::gesture::detector::GestureDetector;
use crate::gesture::filter::ImuFilter;
use crate::ipc::{GESTURE_CHANNEL, IMU_SIGNAL, POWER_SIGNAL};

/// Task 1: Sensor & Gestures (QMI8658 IMU + Power Rail Monitor)
#[allow(
    clippy::large_stack_frames,
    reason = "Sensor calibration loop and attitude filter state"
)]
#[embassy_executor::task]
pub async fn sensor_task(
    mut imu: Qmi8658<SharedI2cDevice<'static>>,
    mut power_key: PowerKeyMonitor<'static>,
) -> ! {
    let mut filter = ImuFilter::new();
    let mut detector = GestureDetector::new();

    // Initial gyro stationary calibration (accumulate sum directly if IMU is available)
    if imu.is_available() {
        let mut sum_gx: i64 = 0;
        let mut sum_gy: i64 = 0;
        let mut sum_gz: i64 = 0;
        let mut valid_samples: i64 = 0;
        for _ in 0..50 {
            if let Ok(sample) = imu.read_sample() {
                sum_gx += sample.gx_mdps as i64;
                sum_gy += sample.gy_mdps as i64;
                sum_gz += sample.gz_mdps as i64;
                valid_samples += 1;
            }
            Timer::after(Duration::from_millis(10)).await;
        }
        if valid_samples > 0 {
            filter.set_gyro_bias(
                (sum_gx / valid_samples) as i32,
                (sum_gy / valid_samples) as i32,
                (sum_gz / valid_samples) as i32,
            );
        }
    }

    let mut last_power_sample = Instant::now();
    let mut last_imu_emit = Instant::now();
    let mut ticker = Ticker::every(Duration::from_millis(5));

    loop {
        ticker.next().await;
        let now = Instant::now();

        // 1. Read 6-DOF sample
        if let Ok(sample) = imu.read_sample() {
            let attitude = filter.update(&sample, 0.005);
            if let Some(event) = detector.process_imu(&sample, &attitude, now)
                && GESTURE_CHANNEL.try_send(event).is_err()
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
                imu_status: imu.status(),
            });
        }

        // 2. Periodic Power Rail sampling (every 200ms)
        if now.duration_since(last_power_sample) >= Duration::from_millis(200) {
            last_power_sample = now;
            POWER_SIGNAL.signal(power_key.read_status());
        }
    }
}
