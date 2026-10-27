//! BLE radio controller runner, scanning, peripheral GATT, and presence tracking tasks.

use core::future::pending;

use embassy_futures::select::{Either3, Either4, select3, select4};
use embassy_time::{Duration, Instant, Ticker, Timer};
use esp_println::println;
use esp_radio::ble::controller::BleConnector;
use trouble_host::prelude::*;

use crate::domain::MusicScene;
use crate::ipc::{
    BLE_ADVERTISEMENT_CHANNEL, HID_ACTION_CHANNEL, MUSIC_SCENE_CHANNEL, RADIO_REPORT_CHANNEL,
    RADIO_STATUS_SIGNAL, RADIO_TARGET_SIGNAL, TARGET_APPLIED_SIGNAL,
};
use crate::radio::hid_session::{
    HidControlCommand, HidQueueOutcome, HidRecovery, HidSession, HidSubscription,
    queue_report_with_timeout,
};
use crate::radio::{
    AdvertisementHandler, BlePresenceTracker, ConsumerControlReport, DigiviceGattServer,
    RadioReport, RadioStatus, TargetBinding, TargetBindingState,
};

const BLE_SCAN_IDLE_TIMEOUT: Duration = Duration::from_secs(1);
const BLE_SCAN_SESSION_DURATION: Duration = Duration::from_secs(30);

fn discard_pending_hid_actions() {
    HID_ACTION_CHANNEL.clear();
}

async fn wait_for_hid_release(at: Option<Instant>) {
    match at {
        Some(at) => Timer::at(at).await,
        None => pending().await,
    }
}

/// Task: BLE HCI host runner.
#[allow(
    clippy::large_stack_frames,
    reason = "Trouble host runner owns the HCI receive buffers"
)]
#[embassy_executor::task]
pub async fn ble_runner_task(
    mut runner: Runner<'static, ExternalController<BleConnector<'static>, 1>, DefaultPacketPool>,
) -> ! {
    println!("BLE HCI host runner started");
    if let Err(err) = runner.run_with_handler(&AdvertisementHandler).await {
        println!("ERROR: BLE HCI host runner exited: {:?}", err);
    } else {
        println!("ERROR: BLE HCI host runner returned unexpectedly");
    }
    RADIO_STATUS_SIGNAL.signal(RadioStatus::RunnerExited);
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

/// Task: BLE scan session lifecycle.
#[embassy_executor::task]
pub async fn ble_scan_task(
    mut central: Central<'static, ExternalController<BleConnector<'static>, 1>, DefaultPacketPool>,
) -> ! {
    println!("BLE scan session task started");
    // Delay initial scan so peripheral advertising can complete its setup first
    Timer::after(Duration::from_secs(2)).await;

    let scan_config = trouble_host::connection::ScanConfig {
        active: false,
        interval: Duration::from_millis(250),
        window: Duration::from_millis(50),
        ..Default::default()
    };

    let mut scanner = Scanner::new(&mut central);
    loop {
        let session = match scanner.scan(&scan_config).await {
            Ok(session) => session,
            Err(err) => {
                println!("WARN: Failed to start BLE scanner: {:?}", err);
                Timer::after(BLE_SCAN_IDLE_TIMEOUT).await;
                continue;
            }
        };
        Timer::after(BLE_SCAN_SESSION_DURATION).await;
        drop(session);
    }
}

/// BLE peripheral task for HID media reports and MusicScene writes.
#[allow(
    clippy::large_stack_frames,
    reason = "Peripheral advertising task owns the BLE connection future and controller buffers"
)]
#[embassy_executor::task]
pub async fn ble_peripheral_task(
    mut peripheral: Peripheral<
        'static,
        ExternalController<BleConnector<'static>, 1>,
        DefaultPacketPool,
    >,
) -> ! {
    Timer::after(Duration::from_secs(1)).await;
    println!("BLE peripheral task initializing GATT server...");
    let server: DigiviceGattServer =
        match DigiviceGattServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
            name: "Digivice",
            appearance: &trouble_host::prelude::appearance::human_interface_device::GENERIC_HUMAN_INTERFACE_DEVICE,
        })) {
            Ok(server) => server,
            Err(err) => {
                println!("ERROR: BLE GATT server initialization failed: {:?}", err);
                RADIO_STATUS_SIGNAL.signal(RadioStatus::GattFailed);
                loop {
                    Timer::after(Duration::from_secs(3600)).await;
                }
            }
        };

    let mut adv_data = [0u8; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(
                trouble_host::prelude::LE_GENERAL_DISCOVERABLE
                    | trouble_host::prelude::BR_EDR_NOT_SUPPORTED,
            ),
            AdStructure::CompleteServiceUuids16(&[[0x12, 0x18]]), // 0x1812: HID Service
            AdStructure::CompleteLocalName(b"Digivice"),
        ],
        &mut adv_data,
    )
    .unwrap_or(0);

    println!(
        "BLE advertising prepared: name='Digivice', adv_len={} bytes",
        adv_len
    );

    loop {
        let advertiser = match peripheral
            .advertise(
                &Default::default(),
                trouble_host::prelude::Advertisement::ConnectableScannableUndirected {
                    adv_data: &adv_data[..adv_len],
                    scan_data: &[],
                },
            )
            .await
        {
            Ok(advertiser) => {
                println!(
                    "BLE advertising ACTIVE as 'Digivice' (0x1812 HID). Waiting for connection..."
                );
                advertiser
            }
            Err(err) => {
                println!("WARN: BLE peripheral advertise failed: {:?}", err);
                Timer::after(Duration::from_secs(1)).await;
                continue;
            }
        };

        discard_pending_hid_actions();
        let connection = match advertiser.accept().await {
            Ok(connection) => connection,
            Err(err) => {
                println!("WARN: BLE peripheral connection failed: {:?}", err);
                Timer::after(Duration::from_millis(250)).await;
                continue;
            }
        };
        let connection = match connection.with_attribute_server(&server) {
            Ok(connection) => connection,
            Err(err) => {
                println!("WARN: BLE GATT connection setup failed: {:?}", err);
                continue;
            }
        };
        discard_pending_hid_actions();
        let mut hid_session = HidSession::new(Instant::now());
        loop {
            let now = Instant::now();
            let subscription = if server.hid.report.should_notify(&connection) {
                HidSubscription::Enabled
            } else {
                HidSubscription::Disabled
            };
            hid_session.set_subscription(subscription, now);

            // A due release outranks both new presses and a continuous stream of GATT events.
            if hid_session.release_due(now) {
                let release = ConsumerControlReport::release_bytes();
                let outcome = queue_report_with_timeout(server.hid.report.notify(
                    &connection,
                    &release,
                    true,
                ))
                .await;
                match outcome {
                    HidQueueOutcome::Queued => {
                        hid_session.release_queued();
                        println!("BLE HID neutral report queued (not a host acknowledgement)");
                        continue;
                    }
                    HidQueueOutcome::Failed(error) => {
                        println!("WARN: BLE HID release queue failed: {:?}", error);
                    }
                    HidQueueOutcome::TimedOut => {
                        println!("WARN: BLE HID release queue timed out");
                    }
                }
                discard_pending_hid_actions();
                match hid_session.release_failed(Instant::now()) {
                    HidRecovery::RetryAt(_) => continue,
                    HidRecovery::Disconnect => {
                        println!("WARN: BLE HID release recovery exhausted; disconnecting");
                        connection.raw().disconnect();
                        break;
                    }
                }
            }

            match select3(
                connection.next(),
                wait_for_hid_release(hid_session.next_release_at()),
                HID_ACTION_CHANNEL.receive(),
            )
            .await
            {
                Either3::First(event) => match event {
                    GattConnectionEvent::Disconnected { reason } => {
                        println!("BLE peripheral disconnected: {:?}", reason);
                        break;
                    }
                    GattConnectionEvent::PassKeyConfirm(key) => {
                        println!("BLE PassKeyConfirm: {}, accepting JustWorks pairing", key);
                        let _ = connection.pass_key_confirm();
                    }
                    GattConnectionEvent::PassKeyDisplay(key) => {
                        println!("BLE PassKeyDisplay: {}", key);
                    }
                    GattConnectionEvent::PairingComplete {
                        security_level,
                        bond,
                    } => {
                        println!(
                            "BLE PairingComplete: level={:?}, bonded={}",
                            security_level,
                            bond.is_some()
                        );
                    }
                    GattConnectionEvent::PairingFailed(err) => {
                        println!("WARN: BLE PairingFailed: {:?}", err);
                    }
                    GattConnectionEvent::Encrypted {
                        security_level,
                        bond,
                    } => {
                        println!(
                            "BLE Encrypted link: level={:?}, bonded={}",
                            security_level,
                            bond.is_some()
                        );
                    }
                    GattConnectionEvent::Gatt { event } => match event {
                        GattEvent::Write(write) if write.handle() == server.music.scene.handle => {
                            let scene = write
                                .with_data(|_, data| MusicScene::from_frame(data, Instant::now()));
                            if let Ok(scene) = scene {
                                let reply = if MUSIC_SCENE_CHANNEL.try_send(scene).is_ok() {
                                    write.accept()
                                } else {
                                    println!("WARN: MUSIC_SCENE_CHANNEL full");
                                    write.reject(
                                        trouble_host::prelude::AttErrorCode::INSUFFICIENT_RESOURCES,
                                    )
                                };
                                if let Ok(reply) = reply
                                    && let Err(error) = reply.try_send()
                                {
                                    println!("WARN: BLE ATT reply queue failed: {:?}", error);
                                    connection.raw().disconnect();
                                    break;
                                }
                            } else if let Ok(reply) =
                                write.reject(trouble_host::prelude::AttErrorCode::VALUE_NOT_ALLOWED)
                                && let Err(error) = reply.try_send()
                            {
                                println!("WARN: BLE ATT reply queue failed: {:?}", error);
                                connection.raw().disconnect();
                                break;
                            }
                        }
                        GattEvent::Write(write)
                            if write.handle() == server.hid.control_point.handle =>
                        {
                            let command =
                                write.with_data(|_, data| HidControlCommand::decode(data));
                            let reply = if let Some(command) = command {
                                let reply = write.accept();
                                if reply.is_ok() {
                                    hid_session.set_power(command.power_state(), Instant::now());
                                    discard_pending_hid_actions();
                                    println!("BLE HID control: {:?}", command);
                                }
                                reply
                            } else {
                                write.reject(trouble_host::prelude::AttErrorCode::VALUE_NOT_ALLOWED)
                            };
                            if let Ok(reply) = reply
                                && let Err(error) = reply.try_send()
                            {
                                println!("WARN: BLE ATT reply queue failed: {:?}", error);
                                connection.raw().disconnect();
                                break;
                            }
                        }
                        GattEvent::Write(write) => {
                            if Some(write.handle()) == server.hid.report.cccd_handle {
                                discard_pending_hid_actions();
                            }
                            if let Ok(reply) = write.accept()
                                && let Err(error) = reply.try_send()
                            {
                                println!("WARN: BLE ATT reply queue failed: {:?}", error);
                                connection.raw().disconnect();
                                break;
                            }
                        }
                        event => {
                            if let Ok(reply) = event.accept()
                                && let Err(error) = reply.try_send()
                            {
                                println!("WARN: BLE ATT reply queue failed: {:?}", error);
                                connection.raw().disconnect();
                                break;
                            }
                        }
                    },
                    _ => {}
                },
                Either3::Second(()) => {}
                Either3::Third(action) => {
                    let report = match hid_session.begin_press(action, Instant::now()) {
                        Ok(report) => report.bytes(),
                        Err(reason) => {
                            println!("WARN: dropped HID action {:?}: {:?}", action, reason);
                            discard_pending_hid_actions();
                            continue;
                        }
                    };
                    match queue_report_with_timeout(server.hid.report.notify(
                        &connection,
                        &report,
                        true,
                    ))
                    .await
                    {
                        HidQueueOutcome::Queued => {
                            println!("BLE HID press queued: {:?}; release still pending", action);
                        }
                        HidQueueOutcome::Failed(error) => {
                            println!("WARN: BLE HID press queue failed: {:?}", error);
                            discard_pending_hid_actions();
                        }
                        HidQueueOutcome::TimedOut => {
                            println!("WARN: BLE HID press queue timed out: {:?}", action);
                            discard_pending_hid_actions();
                        }
                    }
                }
            }
        }
        if let Err(error) = server
            .hid
            .report
            .set(&server, &ConsumerControlReport::release_bytes())
        {
            println!("WARN: BLE HID idle value reset failed: {:?}", error);
        }
        discard_pending_hid_actions();
    }
}

/// Task 8: BLE Presence & Radio Monitor
#[embassy_executor::task]
pub async fn radio_presence_task(
    target: Option<TargetBinding>,
    rssi_away_threshold: i8,
    initial_status: RadioStatus,
) -> ! {
    let mut tracker = BlePresenceTracker::new_with_away_threshold_and_status(
        target,
        rssi_away_threshold,
        initial_status,
    );
    let mut ticker = Ticker::every(Duration::from_millis(1000));
    let mut last_presence = tracker.presence;
    let mut last_visibility = tracker.target_visibility;
    let mut last_rssi = tracker.smoothed_target_rssi as i8;
    let mut last_status = tracker.radio_status;

    loop {
        let next = select4(
            RADIO_STATUS_SIGNAL.wait(),
            RADIO_TARGET_SIGNAL.wait(),
            BLE_ADVERTISEMENT_CHANNEL.receive(),
            ticker.next(),
        )
        .await;

        let is_tick = matches!(next, Either4::Fourth(_));
        match next {
            Either4::First(status) => {
                tracker.set_radio_status(status);
            }
            Either4::Second(command) => {
                tracker.set_target(command.into());
                TARGET_APPLIED_SIGNAL.signal(command);
            }
            Either4::Third(advertisement) => {
                tracker.process_advertisement(
                    &advertisement.addr,
                    advertisement.rssi,
                    Instant::now(),
                );
                // Bound catch-up work so watchdog/report maintenance cannot starve under a flood.
                for _ in 1..8 {
                    let Ok(advertisement) = BLE_ADVERTISEMENT_CHANNEL.try_receive() else {
                        break;
                    };
                    tracker.process_advertisement(
                        &advertisement.addr,
                        advertisement.rssi,
                        Instant::now(),
                    );
                }
            }
            Either4::Fourth(_) => {}
        }

        let now = Instant::now();
        let presence = tracker.update_watchdog(now);
        let rssi = tracker.smoothed_target_rssi as i8;
        let visibility = tracker.target_visibility;
        let status = tracker.radio_status;

        if is_tick {
            let dropped_advs = crate::radio::gatt::ADV_DROPPED_PACKETS
                .swap(0, core::sync::atomic::Ordering::Relaxed);
            if dropped_advs > 0 {
                println!(
                    "BLE scan: dropped {} advertisements in channel overflow",
                    dropped_advs
                );
            }
        }

        let changed = status != last_status
            || presence != last_presence
            || visibility != last_visibility
            || rssi != last_rssi;
        if is_tick || changed {
            last_status = status;
            last_presence = presence;
            last_visibility = visibility;
            last_rssi = rssi;

            let report = RadioReport {
                status,
                target_bound: TargetBindingState::from(tracker.target_hash.is_some()),
                target_seen: tracker.target_visibility,
                presence,
                ble_rssi: rssi,
                ble_blips: tracker.blips.clone(),
            };
            if RADIO_REPORT_CHANNEL.try_send(report).is_err() {
                static REPORT_DROPS: core::sync::atomic::AtomicU32 =
                    core::sync::atomic::AtomicU32::new(0);
                let drops = REPORT_DROPS.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1;
                if drops.is_power_of_two() {
                    println!(
                        "WARN: RADIO_REPORT_CHANNEL full (dropped {} reports)",
                        drops
                    );
                }
            }
        }
    }
}
