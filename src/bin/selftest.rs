//! ESP32-C6 Hardware Self-Test Diagnostic Utility
//!
//! Dedicated hardware verification and factory test harness for Waveshare ESP32-C6-LCD-1.28.
//! Tests GC9A01 LCD via 4-wire SPI, QMI8658 6-DOF IMU, PCF85063 RTC, ETA6098 power rail/charging,
//! and GPIO9 BOOT strapping key. Completely decoupled from production cyberpet firmware.

#![no_std]
#![no_main]
#![deny(warnings)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

extern crate alloc;

use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;

use digivice::board::{Board, BoardPeripherals, ImuSample, ImuStatus};
use digivice::ui::selftest_view::render_selftest;
use digivice::ui::{COLOR_BG, UiState};
use embassy_time::{Duration, Instant, Timer};

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    println!("{}", panic_info);
    loop {}
}

#[allow(
    clippy::large_stack_frames,
    reason = "esp_rtos::main macro expands to runtime startup futures"
)]
#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    println!("==================================================");
    println!("Starting ESP32-C6 Hardware Self-Test Diagnostic...");
    println!("==================================================");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize 128KB Heap
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    println!("Embassy OS runtime started for Hardware Self-Test!");

    // Initialize Board Peripherals
    let mut board = Board::init(BoardPeripherals {
        flash: peripherals.FLASH,
        spi2: peripherals.SPI2,
        ledc: peripherals.LEDC,
        i2c0: peripherals.I2C0,
        adc1: peripherals.ADC1,
        gpio0: peripherals.GPIO0,
        gpio4: peripherals.GPIO4,
        gpio5: peripherals.GPIO5,
        gpio6: peripherals.GPIO6,
        gpio7: peripherals.GPIO7,
        gpio8: peripherals.GPIO8,
        gpio9: peripherals.GPIO9,
        gpio10: peripherals.GPIO10,
        gpio11: peripherals.GPIO11,
        gpio15: peripherals.GPIO15,
        gpio18: peripherals.GPIO18,
        gpio19: peripherals.GPIO19,
        gpio20: peripherals.GPIO20,
        gpio21: peripherals.GPIO21,
        gpio23: peripherals.GPIO23,
    });

    board.backlight.set_brightness(80);
    println!("LCD Backlight set to 80%");

    let mut state = UiState {
        backlight_pct: 80,
        ..Default::default()
    };

    let mut last_log = Instant::now();
    let mut heading_accum: f32 = 0.0;

    println!("Entering hardware diagnostic render loop (~30 FPS)...");

    loop {
        let now = Instant::now();

        // 1. Sample IMU 6-DOF sensors
        match board.imu.read_sample() {
            Ok(sample) => {
                state.imu_sample = sample;
                state.imu_status = ImuStatus::HardwarePresent;
                // Integrate Z gyro into heading angle (dt = 0.033s)
                let mut h = heading_accum + (sample.gz_mdps as f32 / 1000.0) * 0.033;
                while h >= 360.0 {
                    h -= 360.0;
                }
                while h < 0.0 {
                    h += 360.0;
                }
                heading_accum = h;
                state.heading_deg = heading_accum;
            }
            Err(_) => {
                state.imu_status = board.imu.status();
                state.imu_sample = ImuSample::default();
            }
        }

        // 2. Sample RTC time
        state.datetime = board.rtc.get_datetime();
        state.rtc_status = board.rtc.status();

        // 3. Sample Battery & Charging status
        let power_status = board.power_key.read_status();
        state.v_rail_mv = power_status.v_rail_mv;
        state.charge_state = power_status.charge_state;
        state.boot_press_state = power_status.boot_press_state;

        // 4. Render Hardware Self-Test Screen onto 240x240 LCD
        board
            .display
            .render_full_frame(COLOR_BG, |target| {
                render_selftest(target, &state);
            })
            .ok();

        // 5. Periodic Diagnostic Telemetry Log over USB-Serial-JTAG (every 1 second)
        if now.duration_since(last_log) >= Duration::from_millis(1000) {
            last_log = now;
            let boot_str = if state.is_boot_pressed() {
                "PRESSED"
            } else {
                "RELEASED"
            };
            println!(
                "[SELFTEST] IMU: {:?} ACC:({:+05}, {:+05}, {:+05})mg GYR_Z:{:+04}dps | RTC: {:02}:{:02}:{:02} | BAT: {}mV ({}) | BOOT: {}",
                state.imu_status,
                state.imu_sample.ax_mg,
                state.imu_sample.ay_mg,
                state.imu_sample.az_mg,
                state.imu_sample.gz_mdps / 1000,
                state.datetime.hour(),
                state.datetime.minute(),
                state.datetime.second(),
                state.v_rail_mv,
                state.charge_state.as_str(),
                boot_str
            );
        }

        Timer::after(Duration::from_millis(30)).await;
    }
}
