use crate::support;

use std::{cell::RefCell, rc::Rc};

use digivice_core::storage::{
    CONFIG_COMMIT_OFFSET, CONFIG_FLASH_PRIMARY_OFFSET, CONFIG_FLASH_SECONDARY_OFFSET, ConfigSchema,
    ConfigSectorStatus, ConfigSource, DeviceConfig, FLASH_SECTOR_SIZE, SaveCoordinator, SaveStatus,
    StorageError, StorageManager,
};
use digivice_core::storage::{SaveCompletionSignal, SaveRequestSignal};
use embassy_time::Instant;
use embedded_storage::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};

const BASE: u32 = CONFIG_FLASH_PRIMARY_OFFSET;
const CAPACITY: usize = 0x2_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Operation {
    Read(u32, usize),
    Write(u32, usize),
    Erase(u32, u32),
}

#[derive(Clone, Copy)]
struct Fault {
    operation: usize,
    completed_bytes: usize,
}

struct FlashState {
    bytes: Vec<u8>,
    operations: Vec<Operation>,
    fault: Option<Fault>,
}

impl FlashState {
    fn begin(&mut self, operation: Operation, len: usize) -> (usize, bool) {
        self.operations.push(operation);
        match self.fault {
            Some(fault) if fault.operation == self.operations.len() => {
                (fault.completed_bytes.min(len), true)
            }
            _ => (len, false),
        }
    }

    fn reset(&mut self, fault: Option<Fault>) {
        self.operations.clear();
        self.fault = fault;
    }
}

#[derive(Clone)]
struct FakeNor(Rc<RefCell<FlashState>>);

impl FakeNor {
    fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(Rc::new(RefCell::new(FlashState {
            bytes,
            operations: Vec::new(),
            fault: None,
        })))
    }

    fn erased() -> Self {
        Self::from_bytes(vec![0xff; CAPACITY])
    }
}

impl ErrorType for FakeNor {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for FakeNor {
    const READ_SIZE: usize = 4;

    fn read(&mut self, offset: u32, output: &mut [u8]) -> Result<(), Self::Error> {
        assert_eq!(offset % 4, 0);
        assert_eq!(output.len() % 4, 0);
        let mut state = self.0.borrow_mut();
        let (len, failed) = state.begin(Operation::Read(offset, output.len()), output.len());
        let index = (offset - BASE) as usize;
        output[..len].copy_from_slice(&state.bytes[index..index + len]);
        if failed {
            Err(NorFlashErrorKind::Other)
        } else {
            Ok(())
        }
    }

    fn capacity(&self) -> usize {
        BASE as usize + CAPACITY
    }
}

impl NorFlash for FakeNor {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = FLASH_SECTOR_SIZE;

    fn write(&mut self, offset: u32, input: &[u8]) -> Result<(), Self::Error> {
        assert_eq!(offset % 4, 0);
        assert_eq!(input.len() % 4, 0);
        let mut state = self.0.borrow_mut();
        let (len, failed) = state.begin(Operation::Write(offset, input.len()), input.len());
        let index = (offset - BASE) as usize;
        for (destination, byte) in state.bytes[index..index + len].iter_mut().zip(input) {
            assert_eq!(*destination & byte, *byte, "NOR cannot change 0 bits to 1");
            *destination &= byte;
        }
        if failed {
            Err(NorFlashErrorKind::Other)
        } else {
            Ok(())
        }
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        assert_eq!(from as usize % FLASH_SECTOR_SIZE, 0);
        assert_eq!(to - from, FLASH_SECTOR_SIZE as u32);
        let mut state = self.0.borrow_mut();
        let (len, failed) = state.begin(Operation::Erase(from, to), (to - from) as usize);
        let index = (from - BASE) as usize;
        state.bytes[index..index + len].fill(0xff);
        if failed {
            Err(NorFlashErrorKind::Other)
        } else {
            Ok(())
        }
    }
}

fn changed_config() -> DeviceConfig {
    let mut config = DeviceConfig::default();
    config.pet_hunger = 1;
    config.pet_strength = 2;
    config.pet_evolution_stage = 3;
    config.pet_training_count = 42;
    config
}

fn seed_committed() -> (FakeNor, DeviceConfig) {
    let flash = FakeNor::erased();
    let mut manager = StorageManager::new(flash.clone());
    manager.load_config();
    let old = match manager.save_config(&DeviceConfig::default()) {
        Ok(config) => config,
        Err(error) => panic!("failed to seed flash: {error}"),
    };
    flash.0.borrow_mut().reset(None);
    (flash, old)
}

#[test]
fn a_save_has_one_explicit_erase_and_two_disjoint_nor_programs() {
    let (flash, _) = seed_committed();
    let mut manager = StorageManager::new(flash.clone());
    manager.load_config();
    flash.0.borrow_mut().reset(None);
    assert!(manager.save_config(&changed_config()).is_ok());
    assert_eq!(
        flash.0.borrow().operations,
        [
            Operation::Erase(BASE, BASE + FLASH_SECTOR_SIZE as u32),
            Operation::Write(BASE, CONFIG_COMMIT_OFFSET),
            Operation::Read(BASE, CONFIG_COMMIT_OFFSET),
            Operation::Write(BASE + CONFIG_COMMIT_OFFSET as u32, 4),
            Operation::Read(BASE, DeviceConfig::SERIALIZED_SIZE),
        ]
    );
}

#[test]
fn every_payload_and_marker_byte_cut_recovers_only_old_or_new() {
    let (seed, old) = seed_committed();
    let bytes = seed.0.borrow().bytes.clone();
    for (operation, length) in [(2, CONFIG_COMMIT_OFFSET), (4, 4)] {
        for completed_bytes in 0..=length {
            check_cut(
                &bytes,
                old,
                Fault {
                    operation,
                    completed_bytes,
                },
            );
        }
    }
}

#[test]
fn erase_and_verification_failures_preserve_a_recoverable_commit() {
    let (seed, old) = seed_committed();
    let bytes = seed.0.borrow().bytes.clone();
    for completed_bytes in [0, 1, 2048, 4095, 4096] {
        check_cut(
            &bytes,
            old,
            Fault {
                operation: 1,
                completed_bytes,
            },
        );
    }
    for (operation, length) in [
        (3, CONFIG_COMMIT_OFFSET),
        (5, DeviceConfig::SERIALIZED_SIZE),
    ] {
        for completed_bytes in 0..=length {
            check_cut(
                &bytes,
                old,
                Fault {
                    operation,
                    completed_bytes,
                },
            );
        }
    }
}

fn check_cut(seed: &[u8], old: DeviceConfig, fault: Fault) {
    let flash = FakeNor::from_bytes(seed.to_vec());
    let mut manager = StorageManager::new(flash.clone());
    assert_eq!(manager.load_config(), old);
    flash.0.borrow_mut().reset(Some(fault));
    assert!(manager.save_config(&changed_config()).is_err());

    // The original committed sector is never modified, even by a torn target erase.
    let old_index = (CONFIG_FLASH_SECONDARY_OFFSET - BASE) as usize;
    assert_eq!(
        &flash.0.borrow().bytes[old_index..old_index + FLASH_SECTOR_SIZE],
        &seed[old_index..old_index + FLASH_SECTOR_SIZE],
    );
    flash.0.borrow_mut().reset(None);
    let mut reboot = StorageManager::new(flash.clone());
    let recovered = reboot.load_config();
    let mut expected_new = changed_config();
    expected_new.seq = old.seq.wrapping_add(1);
    assert!(recovered == old || recovered == expected_new);

    // An acknowledged retry advances from the recovered sequence, not a RAM guess.
    let saved = match reboot.save_config(&changed_config()) {
        Ok(config) => config,
        Err(error) => panic!("retry failed: {error}"),
    };
    assert_eq!(saved.seq, recovered.seq.wrapping_add(1));
    let mut second_reboot = StorageManager::new(flash);
    assert_eq!(second_reboot.load_config(), saved);
}

#[test]
fn sequence_wrap_is_recovered_across_both_sectors() {
    let flash = FakeNor::erased();
    let mut old = DeviceConfig::default();
    old.seq = u32::MAX;
    flash.0.borrow_mut().bytes[..DeviceConfig::SERIALIZED_SIZE].copy_from_slice(&old.serialize());
    let mut manager = StorageManager::new(flash.clone());
    assert_eq!(manager.load_config(), old);
    let saved = match manager.save_config(&changed_config()) {
        Ok(config) => config,
        Err(error) => panic!("wrap save failed: {error}"),
    };
    assert_eq!(saved.seq, 0);
    let mut reboot = StorageManager::new(flash);
    assert_eq!(reboot.load_config(), saved);
}

#[test]
fn invalid_configuration_does_not_erase_flash() {
    let (flash, _) = seed_committed();
    let mut manager = StorageManager::new(flash.clone());
    manager.load_config();
    flash.0.borrow_mut().reset(None);
    let mut config = DeviceConfig::default();
    config.ble_target_hash = 0x1_0000;
    assert_eq!(
        manager.save_config(&config),
        Err(StorageError::InvalidBleTarget)
    );
    assert!(flash.0.borrow().operations.is_empty());
}

#[test]
fn single_flight_transport_commits_changed_pet_fields_and_recovers_after_reboot() {
    let (flash, loaded) = seed_committed();
    let mut storage = StorageManager::new(flash.clone());
    storage.load_config();
    let requests = SaveRequestSignal::new();
    let completions = SaveCompletionSignal::new();
    let mut saves = SaveCoordinator::new(loaded, Instant::from_secs(0));
    let mut candidate = changed_config();
    candidate.pet_training_count = 300;
    saves.observe(candidate);
    let request = match saves.begin_save(Instant::from_secs(300)) {
        Some(request) => request,
        None => panic!("expected changed pet fields to be saved"),
    };
    requests.signal(request);
    assert!(saves.begin_save(Instant::from_secs(600)).is_none());
    let received = match requests.try_take() {
        Some(request) => request,
        None => panic!("request was not delivered"),
    };
    completions.signal(storage.execute_save(received));
    assert!(matches!(saves.status(), SaveStatus::Saving { .. }));
    let completion = match completions.try_take() {
        Some(completion) => completion,
        None => panic!("completion was not delivered"),
    };
    saves.acknowledge(completion);
    assert_eq!(saves.status(), SaveStatus::Clean { seq: 2 });
    let mut reboot = StorageManager::new(flash);
    let recovered = reboot.load_config();
    assert_eq!(recovered, saves.committed_config());
    assert_eq!(recovered.pet_hunger, 1);
    assert_eq!(recovered.pet_strength, 2);
    assert_eq!(recovered.pet_evolution_stage, 3);
    assert_eq!(recovered.pet_training_count, 300);
}

#[test]
fn migrating_v2_to_v3_is_atomic_at_every_program_and_verification_boundary() {
    let flash = FakeNor::erased();
    let old_bytes = support::v2_record(19, 255);
    let index = (CONFIG_FLASH_SECONDARY_OFFSET - BASE) as usize;
    flash.0.borrow_mut().bytes[index..index + old_bytes.len()].copy_from_slice(&old_bytes);
    let mut storage = StorageManager::new(flash.clone());
    let report = storage.load_config_report();
    assert_eq!(report.source, ConfigSource::Secondary);
    assert_eq!(
        report.secondary,
        ConfigSectorStatus::Valid {
            schema: ConfigSchema::Version2,
            seq: 19
        }
    );
    assert_eq!(report.config.pet_training_count, 255);
    assert_eq!(
        report.config.version(),
        digivice_core::storage::CONFIG_VERSION
    );
    let seed = flash.0.borrow().bytes.clone();
    for (operation, length) in [
        (2, CONFIG_COMMIT_OFFSET),
        (3, CONFIG_COMMIT_OFFSET),
        (4, 4),
        (5, DeviceConfig::SERIALIZED_SIZE),
    ] {
        for completed_bytes in 0..=length {
            check_cut(
                &seed,
                report.config,
                Fault {
                    operation,
                    completed_bytes,
                },
            );
        }
    }
    for completed_bytes in [0, 1, 2048, 4095, 4096] {
        check_cut(
            &seed,
            report.config,
            Fault {
                operation: 1,
                completed_bytes,
            },
        );
    }

    let mut candidate = report.config;
    candidate.pet_training_count = 300;
    assert!(storage.save_config(&candidate).is_ok());
    let mut reboot = StorageManager::new(flash);
    let report = reboot.load_config_report();
    assert_eq!(report.source, ConfigSource::Primary);
    assert_eq!(
        report.primary,
        ConfigSectorStatus::Valid {
            schema: ConfigSchema::Current,
            seq: 20
        }
    );
    assert_eq!(report.config.pet_training_count, 300);
}

#[test]
fn out_of_range_new_fields_are_rejected_before_erasing() {
    let (flash, committed) = seed_committed();
    let mut storage = StorageManager::new(flash.clone());
    storage.load_config();
    flash.0.borrow_mut().reset(None);
    for field in 0..4 {
        let mut invalid = committed;
        match field {
            0 => invalid.pet_age_days = 0,
            1 => invalid.pet_poop_count = 5,
            2 => invalid.pet_energy = 101,
            _ => invalid.pet_weight_g = 0,
        }
        assert_eq!(
            storage.save_config(&invalid),
            Err(StorageError::InvalidConfig)
        );
        assert!(flash.0.borrow().operations.is_empty());
    }
}

#[test]
fn actual_pet_care_state_survives_owner_commit_and_reconstruction() {
    use digivice_core::pet::{DefaultPet, PetBehavior, PetEvent, PetSpecies};

    let (flash, loaded) = seed_committed();
    let mut storage = StorageManager::new(flash.clone());
    storage.load_config();
    let mut pet = DefaultPet::with_full_stats(0, 2, PetSpecies::Greymon, 45, 9, 300);
    let day = match digivice_core::domain::CalendarDay::from_day_count(18_262) {
        Some(day) => day,
        None => panic!("valid archive day rejected"),
    };
    pet.handle(PetEvent::CalendarDay(day), Instant::from_millis(0));
    pet.handle(PetEvent::Tap, Instant::from_millis(0));
    pet.tick(Instant::from_millis(300_123));
    let archive = match pet.archive() {
        Some(archive) => archive,
        None => panic!("pet archive unavailable"),
    };
    assert_eq!(archive.data().progress.digestion_ms(), 123);
    let mut candidate = loaded;
    candidate.set_pet_archive(archive);
    let mut saves = SaveCoordinator::new(loaded, Instant::from_millis(0));
    saves.observe(candidate);
    let request = match saves.begin_save(Instant::from_millis(300_123)) {
        Some(request) => request,
        None => panic!("changed care state was not scheduled"),
    };
    saves.acknowledge(storage.execute_save(request));
    assert_eq!(saves.status(), SaveStatus::Clean { seq: 2 });

    let mut reboot = StorageManager::new(flash);
    let restored_config = reboot.load_config();
    let mut restored =
        DefaultPet::from_archive(restored_config.pet_archive(), Instant::from_millis(0));
    let frame = restored.tick(Instant::from_millis(0));
    assert_eq!(restored.archive(), Some(archive));
    assert_eq!(frame.training_count, 300);
    assert_eq!(frame.care_mistakes, 10);
    assert_eq!(frame.poop_count, 4);
    assert_eq!(frame.affinity, 5);
    assert!(frame.is_sick());
}

#[test]
fn owner_failure_keeps_new_ram_changes_pending_until_a_verified_retry() {
    let (flash, loaded) = seed_committed();
    let mut storage = StorageManager::new(flash.clone());
    storage.load_config();
    let requests = SaveRequestSignal::new();
    let completions = SaveCompletionSignal::new();
    let mut saves = SaveCoordinator::new(loaded, Instant::from_secs(0));
    saves.observe(changed_config());
    if let Some(request) = saves.begin_save(Instant::from_secs(300)) {
        requests.signal(request);
    }
    flash.0.borrow_mut().reset(Some(Fault {
        operation: 4,
        completed_bytes: 2,
    }));
    let received = match requests.try_take() {
        Some(request) => request,
        None => panic!("request was not delivered"),
    };
    completions.signal(storage.execute_save(received));
    let mut newest = changed_config();
    newest.pet_evolution_stage = 4;
    saves.observe(newest);
    if let Some(completion) = completions.try_take() {
        saves.acknowledge(completion);
    }
    assert_eq!(
        saves.status(),
        SaveStatus::Retrying {
            seq: 1,
            error: StorageError::WriteFailed
        }
    );
    assert_eq!(saves.committed_config(), loaded);
    assert!(saves.begin_save(Instant::from_secs(599)).is_none());
    flash.0.borrow_mut().reset(None);
    let retry = match saves.begin_save(Instant::from_secs(600)) {
        Some(request) => request,
        None => panic!("failed save was not retried"),
    };
    saves.acknowledge(storage.execute_save(retry));
    assert_eq!(saves.status(), SaveStatus::Clean { seq: 2 });
    let mut reboot = StorageManager::new(flash);
    assert_eq!(reboot.load_config().pet_evolution_stage, 4);
}

#[test]
fn configuration_load_never_touches_the_disabled_blackbox() {
    let flash = FakeNor::erased();
    let blackbox_index = (digivice_core::storage::BLACKBOX_FLASH_OFFSET - BASE) as usize;
    flash.0.borrow_mut().bytes[blackbox_index..blackbox_index + 64].fill(0);
    let original = flash.0.borrow().bytes.clone();
    let mut storage = StorageManager::new(flash.clone());
    assert_eq!(storage.load_config(), DeviceConfig::default());
    assert_eq!(
        flash.0.borrow().operations,
        [
            Operation::Read(CONFIG_FLASH_PRIMARY_OFFSET, DeviceConfig::SERIALIZED_SIZE),
            Operation::Read(CONFIG_FLASH_SECONDARY_OFFSET, DeviceConfig::SERIALIZED_SIZE),
        ]
    );
    assert_eq!(flash.0.borrow().bytes, original);
}

#[test]
fn factory_empty_flash_has_an_explicit_default_source() {
    let flash = FakeNor::erased();
    let mut storage = StorageManager::new(flash.clone());
    let report = storage.load_config_report();
    assert_eq!(report.source, ConfigSource::Defaults);
    assert_eq!(report.primary, ConfigSectorStatus::Empty);
    assert_eq!(report.secondary, ConfigSectorStatus::Empty);
    assert_eq!(report.config, DeviceConfig::default());
    assert_eq!(flash.0.borrow().operations.len(), 2);
}

#[test]
fn a_read_failure_is_not_reported_as_empty_or_corrupt() {
    let (flash, committed) = seed_committed();
    let mut storage = StorageManager::new(flash.clone());
    flash.0.borrow_mut().reset(Some(Fault {
        operation: 1,
        completed_bytes: 0,
    }));
    let report = storage.load_config_report();
    assert_eq!(report.source, ConfigSource::Secondary);
    assert_eq!(report.config, committed);
    assert_eq!(report.primary, ConfigSectorStatus::ReadFailed);
    assert_eq!(
        report.secondary,
        ConfigSectorStatus::Valid {
            schema: ConfigSchema::Current,
            seq: 1
        }
    );

    flash.0.borrow_mut().reset(Some(Fault {
        operation: 2,
        completed_bytes: 0,
    }));
    let report = storage.load_config_report();
    assert_eq!(report.source, ConfigSource::Defaults);
    assert_eq!(report.primary, ConfigSectorStatus::Empty);
    assert_eq!(report.secondary, ConfigSectorStatus::ReadFailed);
}

#[test]
fn a_bad_crc_is_reported_without_erasing_or_silently_calling_it_factory_empty() {
    let (flash, _) = seed_committed();
    let index = (CONFIG_FLASH_SECONDARY_OFFSET - BASE) as usize;
    flash.0.borrow_mut().bytes[index + 8] ^= 1;
    let original = flash.0.borrow().bytes.clone();
    let mut storage = StorageManager::new(flash.clone());
    let report = storage.load_config_report();
    assert_eq!(report.source, ConfigSource::Defaults);
    assert_eq!(report.primary, ConfigSectorStatus::Empty);
    assert_eq!(
        report.secondary,
        ConfigSectorStatus::Invalid(StorageError::CrcMismatch)
    );
    assert_eq!(flash.0.borrow().operations.len(), 2);
    assert_eq!(flash.0.borrow().bytes, original);
}

#[test]
fn legacy_diagnostics_describe_memory_decode_not_a_completed_disk_migration() {
    let flash = FakeNor::erased();
    let mut legacy = [0; 44];
    legacy[..4].copy_from_slice(&digivice_core::storage::CONFIG_MAGIC.to_le_bytes());
    legacy[28] = (-78_i8) as u8;
    legacy[29] = 80;
    flash.0.borrow_mut().bytes[..44].copy_from_slice(&legacy);
    let original = flash.0.borrow().bytes.clone();
    let mut storage = StorageManager::new(flash.clone());
    let report = storage.load_config_report();
    assert_eq!(report.source, ConfigSource::Primary);
    assert_eq!(
        report.primary,
        ConfigSectorStatus::Valid {
            schema: ConfigSchema::LegacyV1,
            seq: 0
        }
    );
    assert_eq!(report.secondary, ConfigSectorStatus::Empty);
    assert_eq!(
        report.config.version(),
        digivice_core::storage::CONFIG_VERSION
    );
    assert_eq!(flash.0.borrow().operations.len(), 2);
    assert_eq!(flash.0.borrow().bytes, original);
}

#[test]
fn saving_requires_both_sector_reads_to_complete_before_any_erase() {
    let (flash, committed) = seed_committed();
    let mut storage = StorageManager::new(flash.clone());
    assert_eq!(
        storage.save_config(&changed_config()),
        Err(StorageError::RecoveryRequired)
    );
    assert!(flash.0.borrow().operations.is_empty());
    flash.0.borrow_mut().reset(Some(Fault {
        operation: 1,
        completed_bytes: 0,
    }));
    let loaded = storage.load_config_report();
    assert_eq!(loaded.config, committed);
    let saves = SaveCoordinator::from_load(loaded, Instant::from_secs(0));
    assert_eq!(
        saves.status(),
        SaveStatus::Retrying {
            seq: 1,
            error: StorageError::RecoveryRequired
        }
    );
    flash.0.borrow_mut().reset(None);
    assert_eq!(
        storage.save_config(&changed_config()),
        Err(StorageError::RecoveryRequired)
    );
    assert!(flash.0.borrow().operations.is_empty());

    // A deliberate complete reload reestablishes the active-sector invariant.
    assert_eq!(storage.load_config_report().config, committed);
    assert!(storage.save_config(&changed_config()).is_ok());
}
