//! Central application state machine and reactive event coordinator task.

use embassy_futures::select::{Either4, select4};
use embassy_time::{Duration, Instant, Ticker};

use crate::app::AppStateMachine;
use crate::ipc::{
    GESTURE_CHANNEL, IMU_SIGNAL, MUSIC_SCENE_CHANNEL, ORIENTATION_SIGNAL, POWER_SIGNAL,
    RADIO_REPORT_CHANNEL, SAVE_COMPLETION_SIGNAL, SAVE_REQUEST_SIGNAL, TARGET_APPLIED_SIGNAL,
    TIME_SIGNAL, UI_STATE_SIGNAL,
};
use crate::storage::{ConfigLoadReport, SaveCoordinator};

/// Task 5: Central Application State Machine & Reactive Event Coordinator
#[allow(
    clippy::large_stack_frames,
    reason = "Central application coordinator maintains state machine across await points"
)]
#[embassy_executor::task]
pub async fn app_state_task(loaded: ConfigLoadReport) -> ! {
    let mut coordinator = AppStateMachine::new(loaded.config);
    let mut saves = SaveCoordinator::from_load(loaded, Instant::now());
    coordinator.handle_save_status(saves.status());
    let mut ticker = Ticker::every(Duration::from_millis(25));

    loop {
        // Safety and due maintenance take priority over optional event backlog.
        let ready = select4(
            ORIENTATION_SIGNAL.wait(),
            ticker.next(),
            GESTURE_CHANNEL.receive(),
            RADIO_REPORT_CHANNEL.receive(),
        )
        .await;
        let selected_orientation = match &ready {
            Either4::First(orientation) => Some(*orientation),
            _ => None,
        };
        if let Some(orientation) = ORIENTATION_SIGNAL.try_take().or(selected_orientation) {
            coordinator.handle_orientation(orientation, Instant::now());
        }

        match ready {
            Either4::First(_) => {}
            Either4::Third(gesture) => {
                coordinator.handle_gesture(gesture);
                for _ in 1..16 {
                    let Ok(g) = GESTURE_CHANNEL.try_receive() else {
                        break;
                    };
                    coordinator.handle_gesture(g);
                }
            }
            Either4::Fourth(report) => {
                coordinator.handle_radio_report(report);
                for _ in 1..4 {
                    let Ok(r) = RADIO_REPORT_CHANNEL.try_receive() else {
                        break;
                    };
                    coordinator.handle_radio_report(r);
                }
            }
            Either4::Second(_) => {
                // Periodic maintenance tick: drain hardware signals
                for _ in 0..8 {
                    let Ok(scene) = MUSIC_SCENE_CHANNEL.try_receive() else {
                        break;
                    };
                    coordinator.handle_music_scene(scene);
                }

                if let Some(command) = TARGET_APPLIED_SIGNAL.try_take() {
                    coordinator.apply_target_binding(command);
                }

                if let Some(clock) = TIME_SIGNAL.try_take() {
                    coordinator.handle_time(clock, Instant::now());
                }

                if let Some(status) = POWER_SIGNAL.try_take() {
                    coordinator.handle_power(status);
                }

                if let Some(snapshot) = IMU_SIGNAL.try_take() {
                    coordinator.handle_imu(snapshot);
                }

                let now = Instant::now();
                coordinator.tick(now);
                saves.observe(coordinator.current_config());
                if let Some(completion) = SAVE_COMPLETION_SIGNAL.try_take() {
                    saves.acknowledge(completion);
                }
                if let Some(request) = saves.begin_save(now) {
                    SAVE_REQUEST_SIGNAL.signal(request);
                }
                coordinator.handle_save_status(saves.status());
            }
        }

        // Publish snapshot to UI immediately if any event or tick modified state
        if let Some(snapshot) = coordinator.take_ui_snapshot() {
            UI_STATE_SIGNAL.signal(snapshot);
        }
    }
}
