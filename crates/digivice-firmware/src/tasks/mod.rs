//! Asynchronous Embassy Concurrency Tasks for ESP32-C6 Digivice.
//!
//! Organizes all runtime background tasks, keeping the firmware entry point (`digivice.rs`)
//! focused purely on hardware initialization, runtime startup, and task spawning.

pub mod app;
pub mod radio;
pub mod rtc;
pub mod sensor;
pub mod storage;
pub mod ui;

pub use app::app_state_task;
pub use radio::{ble_peripheral_task, ble_runner_task, ble_scan_task, radio_presence_task};
pub use rtc::rtc_power_task;
pub use sensor::sensor_task;
pub use storage::storage_task;
pub use ui::ui_render_task;
