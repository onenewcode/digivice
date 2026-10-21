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
use esp_radio::ble::controller::BleConnector;

use embassy_executor::Spawner;
use embassy_futures::select::{Either, Either4, select, select4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::{channel::Channel, signal::Signal};
use embassy_time::{Duration, Instant, Ticker, Timer};
use esp_println::println;
use trouble_host::prelude::{
    Central, DefaultPacketPool, EventHandler, ExternalController, LeAdvReportsIter,
    LeExtAdvReportsIter, Runner, Scanner,
};

use island::app::AppStateMachine;
use island::board::{
    Board, BoardPeripherals, DateTime, ImuAttitudeSnapshot, PowerStatus,
    display::{Backlight, BandDisplay},
    power_key::PowerKeyMonitor,
};
use island::gesture::GESTURE_CHANNEL;
use island::gesture::detector::GestureDetector;
use island::gesture::filter::ImuFilter;
use island::proto::usb_task::{DEVICE_EVENT_CHANNEL, HOST_PACKET_CHANNEL, UsbRxLink, UsbTxLink};
use island::proto::{ActionState, DeviceEvent, TimeSync};
use island::radio::{
    BLE_ADVERTISEMENT_CHANNEL, BleAdvertisement, BlePresenceTracker, BleTargetCommand,
    RADIO_REPORT_CHANNEL, RADIO_TARGET_SIGNAL, RadioReport, TargetBinding, TargetBindingState,
};
use island::storage::{BlackboxSnapshot, DeviceConfig, StorageManager};
use island::ui::island_view::render_island;
use island::ui::pet_view::render_pet;
use island::ui::selftest_view::render_selftest;
use island::ui::{COLOR_BG, UiState, ViewMode};
use static_cell::StaticCell;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    println!("{}", panic_info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

// Inter-task Signals (Strongly-typed to eliminate Boolean Blindness & tuple ambiguity)
pub static UI_STATE_SIGNAL: Signal<CriticalSectionRawMutex, UiState> = Signal::new();
pub static TIME_SIGNAL: Signal<CriticalSectionRawMutex, DateTime> = Signal::new();
pub static TIME_SYNC_SIGNAL: Signal<CriticalSectionRawMutex, TimeSync> = Signal::new();
pub static POWER_SIGNAL: Signal<CriticalSectionRawMutex, PowerStatus> = Signal::new();
pub static IMU_SIGNAL: Signal<CriticalSectionRawMutex, ImuAttitudeSnapshot> = Signal::new();

static DISPLAY_CELL: StaticCell<BandDisplay<'static>> = StaticCell::new();
static BACKLIGHT_CELL: StaticCell<Backlight<'static>> = StaticCell::new();
static BLE_HOST: StaticCell<
    trouble_host::Stack<'static, ExternalController<BleConnector<'static>, 1>, DefaultPacketPool>,
> = StaticCell::new();
static BLE_HOST_RESOURCES: StaticCell<
    trouble_host::HostResources<
        ExternalController<BleConnector<'static>, 1>,
        DefaultPacketPool,
        1,
        1,
    >,
> = StaticCell::new();
static STORAGE_COMMAND_CHANNEL: Channel<CriticalSectionRawMutex, StorageCommand, 8> =
    Channel::new();
static STORAGE_OUTCOME_CHANNEL: Channel<CriticalSectionRawMutex, StorageOutcome, 8> =
    Channel::new();
static TARGET_APPLIED_SIGNAL: Signal<CriticalSectionRawMutex, BleTargetCommand> = Signal::new();

#[derive(Clone, Copy)]
enum StorageCommand {
    Save(DeviceConfig),
    Dump(BlackboxSnapshot),
}

#[derive(Clone, Copy)]
enum StorageOutcome {
    Saved(DeviceConfig),
    SaveFailed(DeviceConfig),
    Dumped,
}

struct AdvertisementHandler;

impl EventHandler for AdvertisementHandler {
    fn on_adv_reports(&self, reports: LeAdvReportsIter<'_>) {
        for report in reports.flatten() {
            let advertisement = BleAdvertisement {
                addr: report.addr.into_inner(),
                rssi: report.rssi,
            };
            let _ = BLE_ADVERTISEMENT_CHANNEL.try_send(advertisement);
        }
    }

    fn on_ext_adv_reports(&self, reports: LeExtAdvReportsIter<'_>) {
        for report in reports.flatten() {
            let advertisement = BleAdvertisement {
                addr: report.addr.into_inner(),
                rssi: report.rssi,
            };
            let _ = BLE_ADVERTISEMENT_CHANNEL.try_send(advertisement);
        }
    }
}

#[allow(
    clippy::large_stack_frames,
    reason = "esp_rtos::main macro expands to runtime startup futures"
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
    let board = Board::init(BoardPeripherals {
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
    let mut storage = board.storage;
    let dev_config = storage.load_config();
    let initial_target = TargetBinding::try_from(dev_config.ble_target_hash).ok();
    println!(
        "Device configuration loaded (magic=0x{:08X}, seq={})",
        dev_config.magic(),
        dev_config.seq
    );

    // 3. Initialize USB-Serial-JTAG for host communications (direct ownership move, no unsafe steal)
    let usb = UsbSerialJtag::new(peripherals.USB_DEVICE).into_async();
    let (usb_rx, usb_tx) = usb.split();
    let usb_rx_link = UsbRxLink::new(usb_rx);
    let usb_tx_link = UsbTxLink::new(usb_tx);

    // 4. Initialize BLE before moving the radio peripheral into its task.
    let ble_transport = BleConnector::new(peripherals.BT, Default::default())
        .expect("Failed to initialize BLE controller");
    let ble_controller = ExternalController::<_, 1>::new(ble_transport);
    let resources = BLE_HOST_RESOURCES.init(trouble_host::HostResources::new());
    let stack = BLE_HOST.init(trouble_host::new(ble_controller, resources).build());
    let central = stack.central();
    let runner = stack.runner();

    // 5. Spawn Background Tasks with descriptive error boundaries
    spawner.spawn(
        ble_runner_task(runner).expect("Failed to create ble_runner_task: executor pool exhausted"),
    );
    spawner.spawn(
        ble_scan_task(central).expect("Failed to create ble_scan_task: executor pool exhausted"),
    );
    spawner.spawn(
        sensor_task(board.imu, board.power_key)
            .expect("Failed to create sensor_task: executor pool exhausted"),
    );
    spawner.spawn(
        usb_rx_task(usb_rx_link).expect("Failed to create usb_rx_task: executor pool exhausted"),
    );
    spawner.spawn(
        usb_tx_task(usb_tx_link).expect("Failed to create usb_tx_task: executor pool exhausted"),
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
        app_state_task(dev_config)
            .expect("Failed to create app_state_task: executor pool exhausted"),
    );
    spawner.spawn(
        radio_presence_task(initial_target)
            .expect("Failed to create radio_presence_task: executor pool exhausted"),
    );
    spawner.spawn(
        storage_task(storage).expect("Failed to create storage_task: executor pool exhausted"),
    );

    // The entry task has no periodic work; keep it alive while Embassy runs spawned tasks.
    core::future::pending::<()>().await;
    unreachable!("the pending future never completes");
}

/// Task 1: Sensor & Gestures (QMI8658 IMU + Power Rail Monitor)
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
            if let Some(event) = detector.process_imu(&sample, &attitude, now) {
                GESTURE_CHANNEL.send(event).await;
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
                sample: island::board::ImuSample::default(),
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

/// Task 2A: USB-Serial-JTAG RX Ingestion Stream
#[embassy_executor::task]
async fn usb_rx_task(mut rx_link: UsbRxLink<'static>) -> ! {
    loop {
        // Asynchronously suspends when FIFO is empty without starving TX or central coordinator
        rx_link.process_rx_chunk().await;
    }
}

/// Task 2B: USB-Serial-JTAG TX Event Dispatcher
#[embassy_executor::task]
async fn usb_tx_task(mut tx_link: UsbTxLink<'static>) -> ! {
    loop {
        // Wakes up on event arrival and transmits frame immediately
        let evt = DEVICE_EVENT_CHANNEL.receive().await;
        tx_link.send_event(&evt).await;
    }
}

/// Task: Serialize configuration updates through a single flash owner.
#[allow(
    clippy::large_stack_frames,
    reason = "Flash storage and snapshot serialization are owned by this single task"
)]
#[embassy_executor::task]
async fn storage_task(mut storage: StorageManager<'static>) -> ! {
    loop {
        let command = STORAGE_COMMAND_CHANNEL.receive().await;
        match command {
            StorageCommand::Save(config) => match storage.save_config(&config) {
                Ok(saved) => {
                    STORAGE_OUTCOME_CHANNEL
                        .send(StorageOutcome::Saved(saved))
                        .await
                }
                Err(err) => {
                    println!("WARN: Failed to persist device config: {}", err);
                    STORAGE_OUTCOME_CHANNEL
                        .send(StorageOutcome::SaveFailed(config))
                        .await;
                }
            },
            StorageCommand::Dump(snapshot) => match storage.record_snapshot(&snapshot) {
                Ok(_) => STORAGE_OUTCOME_CHANNEL.send(StorageOutcome::Dumped).await,
                Err(err) => println!("WARN: Failed to write blackbox snapshot: {}", err),
            },
        }
    }
}

const BLE_SCAN_IDLE_TIMEOUT: Duration = Duration::from_secs(1);

/// Task: BLE HCI host runner.
#[allow(
    clippy::large_stack_frames,
    reason = "Trouble host runner owns the HCI receive buffers"
)]
#[embassy_executor::task]
async fn ble_runner_task(
    mut runner: Runner<'static, ExternalController<BleConnector<'static>, 1>, DefaultPacketPool>,
) -> ! {
    let _ = runner.run_with_handler(&AdvertisementHandler).await;
    unreachable!("BLE host runner exited");
}

/// Task: BLE scan session lifecycle.
#[embassy_executor::task]
async fn ble_scan_task(
    central: Central<'static, ExternalController<BleConnector<'static>, 1>, DefaultPacketPool>,
) -> ! {
    let scan_config = trouble_host::connection::ScanConfig {
        active: false,
        interval: Duration::from_millis(100),
        window: Duration::from_millis(100),
        filter_duplicates: bt_hci::param::FilterDuplicates::Disabled,
        ..Default::default()
    };

    let mut scanner = Scanner::new(central);
    loop {
        let session = match scanner.scan(&scan_config).await {
            Ok(session) => session,
            Err(err) => {
                println!("WARN: Failed to start BLE scanner: {:?}", err);
                Timer::after(BLE_SCAN_IDLE_TIMEOUT).await;
                continue;
            }
        };
        Timer::after(BLE_SCAN_IDLE_TIMEOUT).await;
        drop(session);
    }
}

#[inline]
fn dispatch_device_event(evt: DeviceEvent) {
    if DEVICE_EVENT_CHANNEL.try_send(evt).is_err() {
        println!("WARN: DEVICE_EVENT_CHANNEL full, dropped event: {}", evt);
    }
}

#[inline]
fn enqueue_storage_command(command: StorageCommand) {
    if STORAGE_COMMAND_CHANNEL.try_send(command).is_err() {
        println!("WARN: STORAGE_COMMAND_CHANNEL full, dropped storage command");
    }
}

/// Task 3: Real-Time Clock & Sedentary Timer
#[embassy_executor::task]
async fn rtc_power_task(
    mut rtc: island::board::Pcf85063<island::board::SharedI2cDevice<'static>>,
) -> ! {
    // Broadcast initial time
    TIME_SIGNAL.signal(rtc.get_datetime());

    loop {
        match select(
            Timer::after(Duration::from_millis(500)),
            TIME_SYNC_SIGNAL.wait(),
        )
        .await
        {
            Either::First(_) => {
                let dt = rtc.get_datetime();
                TIME_SIGNAL.signal(dt);
            }
            Either::Second(sync) => {
                let offset_sec = (sync.tz_offset_min as i64) * 60;
                let total_epoch_with_tz = if offset_sec < 0 {
                    sync.epoch.saturating_sub((-offset_sec) as u64)
                } else {
                    sync.epoch.saturating_add(offset_sec as u64)
                };
                let new_dt = island::board::rtc::epoch_to_datetime(total_epoch_with_tz);
                let _ = rtc.set_datetime(&new_dt);
                // Immediately emit the synchronized time to eliminate sync latency
                TIME_SIGNAL.signal(new_dt);
            }
        }
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
    let mut breath_idx: usize = 0;
    let mut needs_render = true;

    // 1.2Hz Breathing pulse between 25% and 100% using precomputed integer LUT (Zero-FPU)
    // 28 samples * 30ms interval = 840ms period (~1.19Hz)
    const BREATHING_LUT: [u8; 28] = [
        62, 71, 79, 86, 92, 96, 99, 100, 99, 96, 92, 86, 79, 71, 62, 53, 45, 38, 32, 28, 25, 26,
        28, 32, 38, 45, 53, 58,
    ];

    loop {
        // Fetch latest state if updated
        if let Some(new_state) = UI_STATE_SIGNAL.try_take() {
            ui_state = new_state;
            needs_render = true;
        }

        let is_breathing = ui_state.action.state() == ActionState::WaitConfirm;

        // Backlight PWM Management
        if ui_state.is_facedown() {
            backlight.set_brightness(0);
        } else if is_breathing {
            breath_idx = (breath_idx + 1) % BREATHING_LUT.len();
            backlight.set_brightness(BREATHING_LUT[breath_idx]);
        } else {
            backlight.set_brightness(ui_state.backlight_pct);
        }

        // Render Frame based on active ViewMode:
        // Skip heavy SPI rendering when device is face down (screen turned off onto desk)
        if needs_render && !ui_state.is_facedown() {
            let mode = ui_state.mode;
            display.render_full_frame(COLOR_BG, |target| match mode {
                ViewMode::SelfTest => render_selftest(target, &ui_state),
                ViewMode::Island => render_island(target, &ui_state),
                ViewMode::Pet => render_pet(target, &ui_state),
            });
            needs_render = false;
        }

        // Energy-efficient dirty-gated sleep:
        // If fluid animation is required (WaitConfirm pulse or Pet mode), step at 30ms;
        // otherwise, wait reactively for new state arrivals (with 500ms safety timeout for watchdogs).
        if ui_state.requires_fluid_animation() {
            Timer::after(Duration::from_millis(30)).await;
        } else {
            match select(
                UI_STATE_SIGNAL.wait(),
                Timer::after(Duration::from_millis(500)),
            )
            .await
            {
                Either::First(new_state) => {
                    ui_state = new_state;
                    needs_render = true;
                }
                Either::Second(_) => {}
            }
        }
    }
}

/// Task 5: Central Application State Machine & Reactive Event Coordinator
#[allow(
    clippy::large_stack_frames,
    reason = "Central application coordinator maintains state machine across await points"
)]
#[embassy_executor::task]
async fn app_state_task(config: DeviceConfig) -> ! {
    let mut coordinator = AppStateMachine::new(config);
    let mut ticker = Ticker::every(Duration::from_millis(25));

    loop {
        // Event-driven reactive concurrency: wake up immediately upon incoming channel data
        // instead of periodic sleep-polling. Periodic ticker handles time/power/imu and watchdog ticks.
        match select4(
            GESTURE_CHANNEL.receive(),
            HOST_PACKET_CHANNEL.receive(),
            RADIO_REPORT_CHANNEL.receive(),
            ticker.next(),
        )
        .await
        {
            Either4::First(gesture) => {
                if let Some(out_evt) = coordinator.handle_gesture(gesture) {
                    dispatch_device_event(out_evt);
                }
                while let Ok(g) = GESTURE_CHANNEL.try_receive() {
                    if let Some(out_evt) = coordinator.handle_gesture(g) {
                        dispatch_device_event(out_evt);
                    }
                }
            }
            Either4::Second(pkt) => {
                let outcome = coordinator.handle_host_packet(pkt);
                if let Some(sync) = outcome.time_sync {
                    TIME_SYNC_SIGNAL.signal(sync);
                }
                if let Some(target) = outcome.target {
                    enqueue_storage_command(StorageCommand::Save(
                        coordinator.config_with_target(target),
                    ));
                }
                if outcome.dump_logs {
                    enqueue_storage_command(StorageCommand::Dump(coordinator.blackbox_snapshot()));
                }
                while let Ok(p) = HOST_PACKET_CHANNEL.try_receive() {
                    let outcome = coordinator.handle_host_packet(p);
                    if let Some(sync) = outcome.time_sync {
                        TIME_SYNC_SIGNAL.signal(sync);
                    }
                    if let Some(target) = outcome.target {
                        enqueue_storage_command(StorageCommand::Save(
                            coordinator.config_with_target(target),
                        ));
                    }
                    if outcome.dump_logs {
                        enqueue_storage_command(StorageCommand::Dump(
                            coordinator.blackbox_snapshot(),
                        ));
                    }
                }
            }
            Either4::Third(report) => {
                if let Some(event) = coordinator.presence_event(&report) {
                    dispatch_device_event(event);
                }
                coordinator.handle_radio_report(report);
                while let Ok(r) = RADIO_REPORT_CHANNEL.try_receive() {
                    if let Some(event) = coordinator.presence_event(&r) {
                        dispatch_device_event(event);
                    }
                    coordinator.handle_radio_report(r);
                }
            }
            Either4::Fourth(_) => {
                // Periodic maintenance tick: drain hardware signals
                while let Ok(outcome) = STORAGE_OUTCOME_CHANNEL.try_receive() {
                    match outcome {
                        StorageOutcome::Saved(config) => {
                            let target = if config.ble_target_hash == 0 {
                                Some(BleTargetCommand::Clear)
                            } else {
                                TargetBinding::try_from(config.ble_target_hash)
                                    .ok()
                                    .map(BleTargetCommand::Bind)
                            };
                            if let Some(target) = target {
                                RADIO_TARGET_SIGNAL.signal(target);
                            }
                        }
                        StorageOutcome::SaveFailed(config) => {
                            println!(
                                "WARN: Keeping previous BLE target after persistence failure (seq={})",
                                config.seq
                            );
                        }
                        StorageOutcome::Dumped => {
                            dispatch_device_event(DeviceEvent::DumpLogsRequest);
                        }
                    }
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
                if let Some(hb) = coordinator.tick(now) {
                    dispatch_device_event(hb);
                }
            }
        }

        // Publish snapshot to UI immediately if any event or tick modified state
        if let Some(snapshot) = coordinator.take_ui_snapshot() {
            UI_STATE_SIGNAL.signal(snapshot);
        }
    }
}

/// Task 8: BLE Presence & Radio Monitor
#[embassy_executor::task]
async fn radio_presence_task(target: Option<TargetBinding>) -> ! {
    let mut tracker = BlePresenceTracker::new(target);
    let mut ticker = Ticker::every(Duration::from_millis(1000));

    loop {
        let next = embassy_futures::select::select3(
            RADIO_TARGET_SIGNAL.wait(),
            BLE_ADVERTISEMENT_CHANNEL.receive(),
            ticker.next(),
        )
        .await;

        match next {
            embassy_futures::select::Either3::First(command) => {
                tracker.set_target(command.into());
                TARGET_APPLIED_SIGNAL.signal(command);
            }
            embassy_futures::select::Either3::Second(advertisement) => {
                tracker.process_advertisement(
                    &advertisement.addr,
                    advertisement.rssi,
                    Instant::now(),
                );
                while let Ok(advertisement) = BLE_ADVERTISEMENT_CHANNEL.try_receive() {
                    tracker.process_advertisement(
                        &advertisement.addr,
                        advertisement.rssi,
                        Instant::now(),
                    );
                }
            }
            embassy_futures::select::Either3::Third(_) => {}
        }

        let now = Instant::now();
        let presence = tracker.update_watchdog(now);
        let rssi = tracker.smoothed_target_rssi as i8;

        let report = RadioReport {
            target_bound: TargetBindingState::from(tracker.target_hash.is_some()),
            target_seen: tracker.target_visibility,
            presence,
            ble_rssi: rssi,
            ble_blips: tracker.blips.clone(),
        };
        let _ = RADIO_REPORT_CHANNEL.try_send(report);
    }
}
