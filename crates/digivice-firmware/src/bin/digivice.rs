//! ESP32-C6 Digivice Cyberpet Firmware Entry Point
//!
//! Serves as the pure composition root: initializes hardware peripherals via the Board Support
//! Package (BSP), configures the memory heap, loads device settings from flash, instantiates the
//! BLE controller and trouble-host network stack, and launches the runtime Embassy background tasks.

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
use esp_radio::ble::controller::BleConnector;
use static_cell::StaticCell;
use trouble_host::prelude::*;

use digivice::board::display::{Backlight, BandDisplay};
use digivice::board::{Board, BoardPeripherals};
use digivice::radio::{RadioStatus, TargetBinding};
use digivice::tasks::*;

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    println!("{}", panic_info);
    loop {}
}

static DISPLAY_CELL: StaticCell<BandDisplay<'static>> = StaticCell::new();
static BACKLIGHT_CELL: StaticCell<Backlight<'static>> = StaticCell::new();
static BLE_HOST: StaticCell<
    trouble_host::Stack<'static, ExternalController<BleConnector<'static>, 1>, DefaultPacketPool>,
> = StaticCell::new();
// Single active master host connection (PC companion app or smartphone).
static BLE_HOST_RESOURCES: StaticCell<trouble_host::HostResources<DefaultPacketPool, 1, 4>> =
    StaticCell::new();

#[allow(
    clippy::large_stack_frames,
    reason = "esp_rtos::main macro expands to runtime startup futures"
)]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    println!("Starting ESP32-C6 Digivice Cyberpet...");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize 128KB Heap
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    println!("Embassy OS runtime started!");

    // 1. Initialize Board Peripherals
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
    let display = DISPLAY_CELL.init(board.display);
    let backlight = BACKLIGHT_CELL.init(board.backlight);

    // 2. Initialize Flash Storage & load config using atomic double-buffered ping-pong
    let load_started = embassy_time::Instant::now();
    let loaded_config = board.storage.load_config_report();
    let load_us = embassy_time::Instant::now()
        .checked_duration_since(load_started)
        .map_or(0, |elapsed| elapsed.as_micros());
    println!(
        "Config source={:?}; sector A={:?}; sector B={:?}; load={}us",
        loaded_config.source, loaded_config.primary, loaded_config.secondary, load_us
    );
    let dev_config = loaded_config.config;
    let initial_target = TargetBinding::try_from(dev_config.ble_target_hash).ok();
    println!(
        "Device configuration loaded (magic=0x{:08X}, seq={})",
        dev_config.magic(),
        dev_config.seq
    );

    // 3. Initialize BLE before moving the radio peripheral into its task.
    let (ble_stack, radio_status) = match BleConnector::new(peripherals.BT, Default::default()) {
        Ok(ble_transport) => {
            let ble_controller = ExternalController::<_, 1>::new(ble_transport);
            let resources = BLE_HOST_RESOURCES.init(trouble_host::HostResources::new());
            let stack = BLE_HOST.init(
                trouble_host::new(ble_controller, resources)
                    .set_io_capabilities(trouble_host::prelude::IoCapabilities::NoInputNoOutput)
                    .build(),
            );
            (Some(stack), RadioStatus::Active)
        }
        Err(err) => {
            println!(
                "WARN: BLE controller initialization failed: {:?}; running in local offline mode",
                err
            );
            (None, RadioStatus::ControllerUnavailable)
        }
    };

    // 4. Spawn background tasks. USB is intentionally not a runtime dependency.
    spawner.spawn(
        storage_task(board.storage)
            .expect("Failed to create storage_task: executor pool exhausted"),
    );
    if let Some(stack) = ble_stack {
        let central = stack.central();
        let peripheral = stack.peripheral();
        let runner = stack.runner();
        spawner.spawn(
            ble_runner_task(runner)
                .expect("Failed to create ble_runner_task: executor pool exhausted"),
        );
        spawner.spawn(
            ble_scan_task(central)
                .expect("Failed to create ble_scan_task: executor pool exhausted"),
        );
        spawner.spawn(
            ble_peripheral_task(peripheral)
                .expect("Failed to create ble_peripheral_task: executor pool exhausted"),
        );
    }
    spawner.spawn(
        sensor_task(
            board.imu,
            board.power_key,
            dev_config.gyro_bias_mdps,
            dev_config.accel_bias_mg,
        )
        .expect("Failed to create sensor_task: executor pool exhausted"),
    );
    spawner.spawn(
        rtc_power_task(board.rtc)
            .expect("Failed to create rtc_power_task: executor pool exhausted"),
    );
    spawner.spawn(
        ui_render_task(display, backlight)
            .expect("Failed to create ui_render_task: executor pool exhausted"),
    );
    spawner.spawn(
        app_state_task(loaded_config)
            .expect("Failed to create app_state_task: executor pool exhausted"),
    );
    spawner.spawn(
        radio_presence_task(initial_target, dev_config.rssi_away_threshold, radio_status)
            .expect("Failed to create radio_presence_task: executor pool exhausted"),
    );

    // The entry task has no periodic work; keep it alive while Embassy runs spawned tasks.
    core::future::pending::<()>().await;
    unreachable!("the pending future never completes");
}
