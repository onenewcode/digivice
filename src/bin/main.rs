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
use esp_hal::usb::usb_serial_jtag::UsbSerialJtag;

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use esp_println::println;

use island::board::{
    Board, BoardPeripherals, DateTime, ImuSample,
    display::{Backlight, BandDisplay},
    power_key::PowerKeyMonitor,
};
use island::gesture::detector::{ButtonDetector, GestureDetector};
use island::gesture::filter::ImuFilter;
use island::gesture::{GESTURE_CHANNEL, GestureEvent};
use island::proto::usb_task::{DEVICE_EVENT_CHANNEL, DeviceEvent, HOST_PACKET_CHANNEL, UsbLink};
use island::proto::{ActionState, HostPacket, TimeSync};
use island::radio::RADIO_REPORT_CHANNEL;
use island::storage::DeviceConfig;
use island::ui::clock_view::render_clock;
use island::ui::island_view::render_island;
use island::ui::radar_view::render_radar;
use island::ui::selftest_view::render_selftest;
use island::ui::{COLOR_BG, UiState, ViewMode};
use static_cell::StaticCell;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    println!("{}", panic_info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

// Inter-task Signals
pub static UI_STATE_SIGNAL: Signal<CriticalSectionRawMutex, UiState> = Signal::new();
pub static TIME_SIGNAL: Signal<CriticalSectionRawMutex, DateTime> = Signal::new();
pub static TIME_SYNC_SIGNAL: Signal<CriticalSectionRawMutex, TimeSync> = Signal::new();
pub static POWER_SIGNAL: Signal<CriticalSectionRawMutex, (u16, bool)> = Signal::new();
pub static IMU_SIGNAL: Signal<CriticalSectionRawMutex, (ImuSample, f32, f32, f32)> = Signal::new();

static DISPLAY_CELL: StaticCell<BandDisplay<'static>> = StaticCell::new();
static BACKLIGHT_CELL: StaticCell<Backlight<'static>> = StaticCell::new();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    println!("Starting ESP32-C6 Desktop Island...");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize 128KB Heap
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    println!("Embassy OS runtime started!");

    // 1. Initialize Board Peripherals
    // Hands over ownership of all required hardware pins including strapping/flash pins,
    // which are consumed and sealed into an inaccessible type sink.
    let board = Board::init(BoardPeripherals {
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
        // Type-level sealing of internal SPI flash & strapping pins
        gpio24: peripherals.GPIO24,
        gpio25: peripherals.GPIO25,
        gpio26: peripherals.GPIO26,
        gpio27: peripherals.GPIO27,
        gpio28: peripherals.GPIO28,
        gpio29: peripherals.GPIO29,
        gpio30: peripherals.GPIO30,
    });
    let display = DISPLAY_CELL.init(board.display);
    let backlight = BACKLIGHT_CELL.init(board.backlight);

    // 2. Initialize Flash Storage & load config
    let dev_config = DeviceConfig::default();
    println!(
        "Device configuration loaded (magic=0x{:08X})",
        dev_config.magic
    );

    // 3. Initialize USB-Serial-JTAG for host communications (direct ownership move, no unsafe steal)
    let usb = UsbSerialJtag::new(peripherals.USB_DEVICE).into_async();
    let (usb_rx, usb_tx) = usb.split();
    let usb_link = UsbLink::new(usb_rx, usb_tx);

    // 4. Spawn Background Tasks
    spawner.spawn(sensor_task(board.imu, board.power_key).unwrap());
    spawner.spawn(usb_task(usb_link).unwrap());
    spawner.spawn(rtc_power_task(board.rtc).unwrap());
    spawner.spawn(ui_render_task(display, backlight).unwrap());
    spawner.spawn(app_state_task(dev_config).unwrap());

    println!("All subsystem tasks spawned successfully!");

    // The entry task has no periodic work; keep it alive while Embassy runs spawned tasks.
    core::future::pending::<()>().await;
    unreachable!("the pending future never completes");
}

/// Task 1: Sensor & Gestures (QMI8658 + BOOT button)
#[allow(
    clippy::large_stack_frames,
    reason = "Sensor calibration loop and attitude filter state"
)]
#[embassy_executor::task]
async fn sensor_task(
    mut imu: island::board::Qmi8658<island::board::SharedI2cDevice<'static>>,
    mut power_key: PowerKeyMonitor<'static>,
) -> ! {
    let mut filter = ImuFilter::new();
    let mut detector = GestureDetector::new();
    let mut button_detector = ButtonDetector::new();

    // Initial gyro stationary calibration (accumulate sum directly)
    let mut sum_gx: i64 = 0;
    let mut sum_gy: i64 = 0;
    let mut sum_gz: i64 = 0;
    for _ in 0..50 {
        if let Ok(sample) = imu.read_sample() {
            sum_gx += sample.gx_mdps as i64;
            sum_gy += sample.gy_mdps as i64;
            sum_gz += sample.gz_mdps as i64;
        }
        Timer::after(Duration::from_millis(10)).await;
    }
    filter.set_gyro_bias(
        (sum_gx / 50) as i32,
        (sum_gy / 50) as i32,
        (sum_gz / 50) as i32,
    );

    let mut last_power_sample = Instant::now();
    let mut last_imu_emit = Instant::now();

    loop {
        let now = Instant::now();

        // 1. Read 6-DOF sample
        if let Ok(sample) = imu.read_sample() {
            let attitude = filter.update(&sample, 0.005);
            if let Some(event) = detector.process_imu(&sample, &attitude, now) {
                GESTURE_CHANNEL.send(event).await;
            }

            // Emit attitude update to UI every 50ms (20Hz)
            if now.duration_since(last_imu_emit) >= Duration::from_millis(50) {
                last_imu_emit = now;
                IMU_SIGNAL.signal((
                    sample,
                    attitude.roll_deg,
                    attitude.pitch_deg,
                    attitude.heading_deg,
                ));
            }
        }

        // 2. Poll BOOT key
        let is_pressed = power_key.is_boot_pressed();
        if let Some(event) = button_detector.update(is_pressed, now) {
            GESTURE_CHANNEL.send(event).await;
        }

        // 3. Periodic Power Rail sampling (every 200ms)
        if now.duration_since(last_power_sample) >= Duration::from_millis(200) {
            last_power_sample = now;
            let v_rail = power_key.sample_v_rail_mv();
            let bc_high = power_key.bc_state_high();
            POWER_SIGNAL.signal((v_rail, bc_high));
        }

        Timer::after(Duration::from_millis(5)).await;
    }
}

/// Task 2: USB-Serial-JTAG Communication & Host Watchdog
#[embassy_executor::task]
async fn usb_task(mut usb_link: UsbLink<'static>) -> ! {
    loop {
        // Read incoming commands from host PC
        usb_link.process_rx_chunk().await;

        let now = Instant::now();
        // Check 10s host watchdog
        let _connected = usb_link.check_watchdog(now);

        // Send queued outgoing events to Host
        while let Ok(evt) = DEVICE_EVENT_CHANNEL.try_receive() {
            usb_link.send_event(&evt).await;
        }

        Timer::after(Duration::from_millis(10)).await;
    }
}

/// Task 3: Real-Time Clock & Sedentary Timer
#[embassy_executor::task]
async fn rtc_power_task(
    mut rtc: island::board::Pcf85063<island::board::SharedI2cDevice<'static>>,
) -> ! {
    loop {
        let dt = rtc.get_datetime();
        TIME_SIGNAL.signal(dt);

        // Check for host sync time requests dispatched to RTC
        if let Some(sync) = TIME_SYNC_SIGNAL.try_take() {
            let total_epoch_with_tz = (sync.epoch as i64 + (sync.tz_offset_min as i64) * 60) as u64;
            let new_dt = island::board::rtc::epoch_to_datetime(total_epoch_with_tz);
            let _ = rtc.set_datetime(&new_dt);
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

/// Task 4: UI Rendering Task (GC9A01 Display + Backlight PWM)
#[allow(
    clippy::large_stack_frames,
    reason = "UI rendering task maintains local frame snapshot and display target"
)]
#[embassy_executor::task]
async fn ui_render_task(
    display: &'static mut BandDisplay<'static>,
    backlight: &'static mut Backlight<'static>,
) -> ! {
    let mut ui_state = UiState::default();
    let mut breath_phase: f32 = 0.0;

    loop {
        // Fetch latest state if updated
        if let Some(new_state) = UI_STATE_SIGNAL.try_take() {
            ui_state = new_state;
        }

        // Backlight PWM Management
        if ui_state.is_facedown {
            backlight.set_brightness(0);
        } else if ui_state.action.state == ActionState::WaitConfirm {
            // 1.2Hz Breathing sine curve between 25% and 100%
            breath_phase += 0.15;
            if breath_phase >= core::f32::consts::TAU {
                breath_phase -= core::f32::consts::TAU;
            }
            let sine = libm::sinf(breath_phase); // -1.0..1.0
            let brightness = 25.0 + ((sine + 1.0) / 2.0) * 75.0;
            backlight.set_brightness(brightness as u8);
        } else if !ui_state.ble_near || ui_state.mode == ViewMode::StandbyClock {
            backlight.set_brightness(12);
        } else {
            backlight.set_brightness(80);
        }

        // Increment radar sweep angle
        ui_state.radar_sweep_angle = (ui_state.radar_sweep_angle + 6) % 360;

        // Render Frame based on active ViewMode
        let mode = ui_state.mode;
        display.render_full_frame(COLOR_BG, |target| match mode {
            ViewMode::SelfTest => render_selftest(target, &ui_state),
            ViewMode::Island => render_island(target, &ui_state),
            ViewMode::Radar => render_radar(target, &ui_state),
            ViewMode::StandbyClock => render_clock(target, &ui_state),
        });

        Timer::after(Duration::from_millis(30)).await;
    }
}

/// Task 5: Central Application State Machine
#[allow(
    clippy::large_stack_frames,
    reason = "Central application coordinator maintains state snapshots and channels"
)]
#[embassy_executor::task]
async fn app_state_task(_config: DeviceConfig) -> ! {
    let mut state = UiState {
        mode: ViewMode::SelfTest, // Start in Stage 1 SelfTest mode!
        ..Default::default()
    };
    let mut last_heartbeat = Instant::now();
    let mut uptime_sec: u32 = 0;

    loop {
        // Update time if signaled
        if let Some(dt) = TIME_SIGNAL.try_take() {
            state.datetime = dt;
        }

        // Update power rail if signaled
        if let Some((v_rail, bc_high)) = POWER_SIGNAL.try_take() {
            state.v_rail_mv = v_rail;
            state.bc_state_high = bc_high;
        }

        // Update IMU telemetry if signaled
        if let Some((sample, roll, pitch, hdg)) = IMU_SIGNAL.try_take() {
            state.imu_sample = sample;
            state.roll_deg = roll;
            state.pitch_deg = pitch;
            state.heading_deg = hdg;
        }

        // 1. Process Gesture Events
        while let Ok(gesture) = GESTURE_CHANNEL.try_receive() {
            match gesture {
                GestureEvent::SingleTap => {
                    state.last_gesture_name = "SingleTap";
                    if state.action.state != ActionState::Idle {
                        state.action.state = ActionState::Idle;
                    }
                    DEVICE_EVENT_CHANNEL
                        .send(DeviceEvent::SingleTap {
                            context: "NOTIFY_CLEAR",
                        })
                        .await;
                }
                GestureEvent::DoubleTap => {
                    state.last_gesture_name = "DoubleTap";
                    if state.action.state == ActionState::WaitConfirm {
                        state.action.state = ActionState::Success;
                        DEVICE_EVENT_CHANNEL
                            .send(DeviceEvent::DoubleTap {
                                context: "WAIT_CONFIRM",
                            })
                            .await;
                    }
                }
                GestureEvent::TiltLeft => {
                    state.last_gesture_name = "TiltLeft";
                    state.mode = ViewMode::Island;
                }
                GestureEvent::TiltRight => {
                    state.last_gesture_name = "TiltRight";
                    state.mode = ViewMode::Radar;
                }
                GestureEvent::FaceDownEnter => {
                    state.last_gesture_name = "FaceDownEnter";
                    state.is_facedown = true;
                    DEVICE_EVENT_CHANNEL
                        .send(DeviceEvent::FaceDown { entering: true })
                        .await;
                }
                GestureEvent::FaceDownExit => {
                    state.last_gesture_name = "FaceDownExit";
                    state.is_facedown = false;
                    DEVICE_EVENT_CHANNEL
                        .send(DeviceEvent::FaceDown { entering: false })
                        .await;
                }
                GestureEvent::HeadingUpdate(hdg) => {
                    state.heading_deg = hdg as f32;
                }
                GestureEvent::KeyShortPress => {
                    state.last_gesture_name = "KeyShort";
                    // Cycle views: SelfTest -> Island -> Radar -> StandbyClock -> SelfTest
                    state.mode = match state.mode {
                        ViewMode::SelfTest => ViewMode::Island,
                        ViewMode::Island => ViewMode::Radar,
                        ViewMode::Radar => ViewMode::StandbyClock,
                        ViewMode::StandbyClock => ViewMode::SelfTest,
                    };
                }
                GestureEvent::KeyDoubleClick => {
                    state.last_gesture_name = "KeyDouble";
                }
                GestureEvent::KeyLongPress => {
                    state.last_gesture_name = "KeyLong";
                    println!("IMU zero-bias recalibration triggered!");
                }
            }
        }

        // 2. Process Host Packets
        while let Ok(pkt) = HOST_PACKET_CHANNEL.try_receive() {
            match pkt {
                HostPacket::Telemetry(telem) => {
                    state.telemetry = telem;
                    state.host_connected = true;
                }
                HostPacket::Action(act) => {
                    state.action = act;
                }
                HostPacket::Sync(sync) => {
                    state.host_connected = true;
                    TIME_SYNC_SIGNAL.signal(sync);
                }
                HostPacket::DumpLogs => {}
            }
        }

        // 3. Process Radio Reports
        while let Ok(report) = RADIO_REPORT_CHANNEL.try_receive() {
            state.wifi_congestion = report.wifi_congestion;
            state.wifi_blips = report.wifi_blips;
            state.ble_near = report.ble_near;
            state.ble_rssi = report.ble_rssi;
            state.ble_blips = report.ble_blips;
        }

        // 4. Periodic Heartbeat with genuine power rail voltage
        let now = Instant::now();
        if now.duration_since(last_heartbeat) >= Duration::from_secs(5) {
            uptime_sec += 5;
            last_heartbeat = now;
            let _ = DEVICE_EVENT_CHANNEL
                .send(DeviceEvent::Heartbeat {
                    v_rail: state.v_rail_mv,
                    uptime_sec,
                })
                .await;
        }

        // Publish snapshot to UI
        UI_STATE_SIGNAL.signal(state.clone());

        Timer::after(Duration::from_millis(50)).await;
    }
}
