use digivice_core::storage::{
    AckDisposition, DeviceConfig, SaveCompletion, SaveCoordinator, SaveRequest, SaveStatus,
    StorageError,
};
use embassy_time::Instant;

fn at(seconds: u64) -> Instant {
    Instant::from_secs(seconds)
}

fn changed(training: u8) -> DeviceConfig {
    let mut config = DeviceConfig::default();
    config.pet_training_count = u16::from(training);
    config
}

fn request(coordinator: &mut SaveCoordinator, seconds: u64) -> SaveRequest {
    match coordinator.begin_save(at(seconds)) {
        Some(request) => request,
        None => panic!("expected a save request at {seconds}"),
    }
}

fn success(request: SaveRequest, seq: u32) -> SaveCompletion {
    let mut config = request.config;
    config.seq = seq;
    SaveCompletion {
        id: request.id,
        result: Ok(config),
    }
}

#[test]
fn changes_are_coalesced_and_unchanged_state_never_writes() {
    let mut save = SaveCoordinator::new(DeviceConfig::default(), at(0));
    assert!(save.begin_save(at(300)).is_none());
    for seconds in 0..300 {
        save.observe(changed((seconds % 255) as u8));
        assert!(save.begin_save(at(seconds)).is_none());
    }
    save.observe(changed(42));
    let pending = request(&mut save, 300);
    assert_eq!(pending.config.pet_training_count, 42);
    assert!(matches!(save.status(), SaveStatus::Saving { seq: 0 }));
    assert_eq!(save.committed_config(), DeviceConfig::default());
    assert_eq!(
        save.acknowledge(success(pending, 1)),
        AckDisposition::Accepted
    );
    assert_eq!(save.status(), SaveStatus::Clean { seq: 1 });
    assert!(save.begin_save(at(3600)).is_none());
}

#[test]
fn changes_during_an_in_flight_write_are_not_acknowledged_by_its_completion() {
    let mut save = SaveCoordinator::new(DeviceConfig::default(), at(0));
    save.observe(changed(1));
    let first = request(&mut save, 300);
    save.observe(changed(2));
    assert!(save.begin_save(at(900)).is_none());
    save.acknowledge(success(first, 1));
    assert_eq!(save.status(), SaveStatus::Dirty { seq: 1 });
    let second = request(&mut save, 900);
    assert_eq!(second.config.pet_training_count, 2);
    assert_eq!(save.acknowledge(success(first, 1)), AckDisposition::Stale);
    assert!(matches!(save.status(), SaveStatus::Saving { seq: 1 }));
    save.acknowledge(success(second, 2));
    assert_eq!(save.status(), SaveStatus::Clean { seq: 2 });
}

#[test]
fn failed_saves_retry_at_the_same_wear_limit_without_advancing_committed_state() {
    let mut save = SaveCoordinator::new(DeviceConfig::default(), at(0));
    save.observe(changed(1));
    let first = request(&mut save, 300);
    save.acknowledge(SaveCompletion {
        id: first.id,
        result: Err(StorageError::WriteFailed),
    });
    assert_eq!(
        save.status(),
        SaveStatus::Retrying {
            seq: 0,
            error: StorageError::WriteFailed
        }
    );
    assert_eq!(save.committed_config(), DeviceConfig::default());
    assert!(save.begin_save(at(599)).is_none());
    let retry = request(&mut save, 600);
    assert_ne!(first.id, retry.id);
    save.acknowledge(success(retry, 1));
    assert_eq!(save.status(), SaveStatus::Clean { seq: 1 });
}

#[test]
fn an_uncertain_commit_requires_repair_even_if_ram_reverts_to_the_old_value() {
    let initial = DeviceConfig::default();
    let mut save = SaveCoordinator::new(initial, at(0));
    save.observe(changed(1));
    let pending = request(&mut save, 300);
    save.observe(initial);
    save.acknowledge(SaveCompletion {
        id: pending.id,
        result: Err(StorageError::ReadFailed),
    });
    assert_eq!(
        save.status(),
        SaveStatus::Retrying {
            seq: 0,
            error: StorageError::ReadFailed
        }
    );
    let repair = request(&mut save, 600);
    assert_eq!(repair.config, initial);
    save.acknowledge(success(repair, 2));
    assert_eq!(save.status(), SaveStatus::Clean { seq: 2 });
}

#[test]
fn an_ack_for_the_wrong_payload_cannot_mark_ram_durable() {
    let mut save = SaveCoordinator::new(DeviceConfig::default(), at(0));
    save.observe(changed(1));
    let pending = request(&mut save, 300);
    save.acknowledge(SaveCompletion {
        id: pending.id,
        result: Ok(changed(2)),
    });
    assert_eq!(
        save.status(),
        SaveStatus::Retrying {
            seq: 0,
            error: StorageError::WriteFailed
        }
    );
}

#[test]
fn continuous_changes_cannot_exceed_288_save_attempts_per_day() {
    let mut save = SaveCoordinator::new(DeviceConfig::default(), at(0));
    let mut attempts = 0;
    for seconds in 0..=86_400 {
        save.observe(changed(((seconds / 300) % 255) as u8));
        if let Some(pending) = save.begin_save(at(seconds)) {
            attempts += 1;
            save.acknowledge(success(pending, attempts));
        }
    }
    assert_eq!(attempts, 288);
}

#[test]
fn deadline_overflow_never_turns_into_an_unlimited_retry_loop() {
    let end = Instant::from_ticks(u64::MAX);
    let mut save = SaveCoordinator::new(DeviceConfig::default(), at(0));
    save.observe(changed(1));
    let pending = match save.begin_save(end) {
        Some(request) => request,
        None => panic!("expected the last representable save"),
    };
    save.acknowledge(SaveCompletion {
        id: pending.id,
        result: Err(StorageError::ReadFailed),
    });
    assert!(save.begin_save(end).is_none());
}
