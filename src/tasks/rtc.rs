//! Periodic Real-Time Clock polling task.

use embassy_time::{Duration, Timer};

use crate::board::{Pcf85063, SharedI2cDevice};
use crate::ipc::TIME_SIGNAL;

/// Task 3: Real-Time Clock & Sedentary Timer
#[embassy_executor::task]
pub async fn rtc_power_task(mut rtc: Pcf85063<SharedI2cDevice<'static>>) -> ! {
    // Broadcast initial time
    TIME_SIGNAL.signal(rtc.get_datetime());

    loop {
        Timer::after(Duration::from_millis(500)).await;
        TIME_SIGNAL.signal(rtc.get_datetime());
    }
}
