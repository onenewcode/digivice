//! BLE radio controller runner, scanning, peripheral GATT, and presence tracking tasks.

use embassy_futures::select::{Either, Either3, select, select3};
use embassy_time::{Duration, Instant, Ticker, Timer};
use esp_println::println;
use esp_radio::ble::controller::BleConnector;
use trouble_host::prelude::*;

use crate::domain::MusicScene;
use crate::ipc::{
    BLE_ADVERTISEMENT_CHANNEL, HID_ACTION_CHANNEL, MUSIC_SCENE_CHANNEL, RADIO_REPORT_CHANNEL,
    RADIO_TARGET_SIGNAL, TARGET_APPLIED_SIGNAL,
};
use crate::radio::{
    AdvertisementHandler, BlePresenceTracker, ConsumerControlReport, IslandGattServer, RadioReport,
    TargetBinding, TargetBindingState,
};

const BLE_SCAN_IDLE_TIMEOUT: Duration = Duration::from_secs(1);

fn discard_pending_hid_actions() {
    HID_ACTION_CHANNEL.clear();
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
        panic!("BLE HCI host runner exited: {:?}", err);
    }
    panic!("BLE HCI host runner returned unexpectedly");
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
        Timer::after(BLE_SCAN_IDLE_TIMEOUT).await;
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
    let server: IslandGattServer =
        match IslandGattServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
            name: "IslandPet",
            appearance: &trouble_host::prelude::appearance::human_interface_device::GENERIC_HUMAN_INTERFACE_DEVICE,
        })) {
            Ok(server) => server,
            Err(err) => {
                panic!("BLE GATT server initialization failed: {:?}", err);
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
            AdStructure::CompleteLocalName(b"IslandPet"),
        ],
        &mut adv_data,
    )
    .unwrap_or(0);

    println!(
        "BLE advertising prepared: name='IslandPet', adv_len={} bytes",
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
                    "BLE advertising ACTIVE as 'IslandPet' (0x1812 HID). Waiting for connection..."
                );
                advertiser
            }
            Err(err) => {
                println!("WARN: BLE peripheral advertise failed: {:?}", err);
                Timer::after(Duration::from_secs(1)).await;
                continue;
            }
        };

        let connection = match select(advertiser.accept(), HID_ACTION_CHANNEL.receive()).await {
            Either::First(Ok(connection)) => connection,
            Either::First(Err(err)) => {
                println!("WARN: BLE peripheral connection failed: {:?}", err);
                Timer::after(Duration::from_millis(250)).await;
                continue;
            }
            Either::Second(_) => {
                println!("WARN: dropped HID action while no client was connected");
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
        loop {
            match select(connection.next(), HID_ACTION_CHANNEL.receive()).await {
                Either::First(event) => match event {
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
                                if let Ok(reply) = reply {
                                    reply.send().await;
                                }
                            } else if let Ok(reply) =
                                write.reject(trouble_host::prelude::AttErrorCode::VALUE_NOT_ALLOWED)
                            {
                                reply.send().await;
                            }
                        }
                        GattEvent::Write(write) => {
                            if let Ok(reply) = write.accept() {
                                reply.send().await;
                            }
                        }
                        event => {
                            if let Ok(reply) = event.accept() {
                                reply.send().await;
                            }
                        }
                    },
                    _ => {}
                },
                Either::Second(action) => {
                    println!("BLE sending HID media action: {:?}", action);
                    let report = ConsumerControlReport::from_action(action).bytes();
                    if server
                        .hid
                        .report
                        .notify(&connection, &report, true)
                        .await
                        .is_ok()
                    {
                        let release = ConsumerControlReport::release_bytes();
                        let _ = server.hid.report.notify(&connection, &release, true).await;
                    }
                    println!("BLE HID action completed: {:?}", action);
                }
            }
        }
        discard_pending_hid_actions();
    }
}

/// Task 8: BLE Presence & Radio Monitor
#[embassy_executor::task]
pub async fn radio_presence_task(target: Option<TargetBinding>) -> ! {
    let mut tracker = BlePresenceTracker::new(target);
    let mut ticker = Ticker::every(Duration::from_millis(1000));

    loop {
        let next = select3(
            RADIO_TARGET_SIGNAL.wait(),
            BLE_ADVERTISEMENT_CHANNEL.receive(),
            ticker.next(),
        )
        .await;

        match next {
            Either3::First(command) => {
                tracker.set_target(command.into());
                TARGET_APPLIED_SIGNAL.signal(command);
            }
            Either3::Second(advertisement) => {
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
            Either3::Third(_) => {}
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
        if RADIO_REPORT_CHANNEL.try_send(report).is_err() {
            println!("WARN: RADIO_REPORT_CHANNEL full");
        }
    }
}
