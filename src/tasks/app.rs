//! Central application state machine and reactive event coordinator task.

use embassy_futures::select::{Either3, select3};
use embassy_time::{Duration, Instant, Ticker};

use crate::app::AppStateMachine;
use crate::ipc::{
    GESTURE_CHANNEL, IMU_SIGNAL, MUSIC_SCENE_CHANNEL, POWER_SIGNAL, RADIO_REPORT_CHANNEL,
    TARGET_APPLIED_SIGNAL, TIME_SIGNAL, UI_STATE_SIGNAL,
};
use crate::storage::DeviceConfig;

/// Task 5: Central Application State Machine & Reactive Event Coordinator
#[allow(
    clippy::large_stack_frames,
    reason = "Central application coordinator maintains state machine across await points"
)]
#[embassy_executor::task]
pub async fn app_state_task(config: DeviceConfig) -> ! {
    let mut coordinator = AppStateMachine::new(config);
    let mut ticker = Ticker::every(Duration::from_millis(25));

    loop {
        // Event-driven reactive concurrency: wake up immediately upon incoming channel data
        // instead of periodic sleep-polling. Periodic ticker handles time/power/imu and watchdog ticks.
        match select3(
            GESTURE_CHANNEL.receive(),
            RADIO_REPORT_CHANNEL.receive(),
            ticker.next(),
        )
        .await
        {
            Either3::First(gesture) => {
                coordinator.handle_gesture(gesture);
                while let Ok(g) = GESTURE_CHANNEL.try_receive() {
                    coordinator.handle_gesture(g);
                }
            }
            Either3::Second(report) => {
                coordinator.handle_radio_report(report);
                while let Ok(r) = RADIO_REPORT_CHANNEL.try_receive() {
                    coordinator.handle_radio_report(r);
                }
            }
            Either3::Third(_) => {
                // Periodic maintenance tick: drain hardware signals
                while let Ok(scene) = MUSIC_SCENE_CHANNEL.try_receive() {
                    coordinator.handle_music_scene(scene);
                }

                if let Some(command) = TARGET_APPLIED_SIGNAL.try_take() {
                    coordinator.apply_target_binding(command);
                }

                if let Some(dt) = TIME_SIGNAL.try_take() {
                    coordinator.handle_time(dt);
                }

                if let Some(status) = POWER_SIGNAL.try_take() {
                    coordinator.handle_power(status);
                }

                if let Some(snapshot) = IMU_SIGNAL.try_take() {
                    coordinator.handle_imu(snapshot);
                }

                let now = Instant::now();
                coordinator.tick(now);
            }
        }

        // Publish snapshot to UI immediately if any event or tick modified state
        if let Some(snapshot) = coordinator.take_ui_snapshot() {
            UI_STATE_SIGNAL.signal(snapshot);
        }
    }
}
