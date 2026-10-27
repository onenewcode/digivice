use digivice_core::domain::{
    CalendarDay, DateTime, HealthStatus, NurtureProgress, PetArchive, PetArchiveData,
};
use digivice_core::pet::{DefaultPet, PetBehavior, PetEvent, PetSpecies};
use digivice_core::storage::DeviceConfig;
use embassy_time::Instant;

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

fn archive(pet: &DefaultPet) -> PetArchive {
    match pet.archive() {
        Some(archive) => archive,
        None => panic!("default pet must provide a durable archive"),
    }
}

fn reboot(pet: &DefaultPet, millis: u64) -> DefaultPet {
    let mut config = DeviceConfig::default();
    config.set_pet_archive(archive(pet));
    let decoded = match DeviceConfig::deserialize(&config.serialize()) {
        Ok(config) => config,
        Err(error) => panic!("archive round trip failed: {error}"),
    };
    DefaultPet::from_archive(decoded.pet_archive(), at(millis))
}

#[test]
fn all_durable_attributes_survive_the_actual_config_and_pet_restore_path() {
    let date = match DateTime::new(2035, 12, 31, 12, 0, 0) {
        Ok(date) => date,
        Err(error) => panic!("invalid test date: {error}"),
    };
    let expected = PetArchive::new_clamped(PetArchiveData {
        stage: 3,
        hunger: 0,
        strength: 2,
        weight_g: 45,
        care_mistakes: 9,
        training_count: 300,
        age_days: 400,
        health: HealthStatus::Sick,
        poop_count: 3,
        energy: 37,
        affinity: 60_000,
        progress: NurtureProgress::new_clamped(15_000, 30_000, 30_000, 170_000),
        calendar_day: Some(CalendarDay::from_datetime(&date)),
    });
    let original = DefaultPet::from_archive(expected, at(0));
    let mut restored = reboot(&original, 1_000_000);
    let frame = restored.tick(at(1_000_000));
    assert_eq!(frame.species, PetSpecies::MetalGreymon);
    assert_eq!(frame.training_count, 300);
    assert_eq!(frame.age_days, 400);
    assert!(frame.is_sick());
    assert!(frame.is_calling());
    assert_eq!(frame.poop_count, 3);
    assert_eq!(frame.energy, 37);
    assert_eq!(frame.affinity, 60_000);
    assert_eq!(archive(&restored), expected);
}

#[test]
fn digestion_resumes_its_remainder_without_counting_downtime_or_old_instants() {
    let mut original = DefaultPet::new();
    original.tick(at(0));
    original.tick(at(29_000));
    assert_eq!(archive(&original).data().progress.digestion_ms(), 29_000);
    for boot in [0, 1_000_000] {
        let mut restored = reboot(&original, boot);
        assert_eq!(restored.tick(at(boot)).hunger, 4);
        assert_eq!(restored.tick(at(boot + 999)).hunger, 4);
        assert_eq!(restored.tick(at(boot + 1000)).hunger, 3);
        assert_eq!(archive(&restored).data().progress.digestion_ms(), 0);
    }
}

#[test]
fn starvation_continues_after_reboot_instead_of_resetting_or_counting_offline_time() {
    let mut original = DefaultPet::with_stats(0, 4);
    original.tick(at(0));
    assert!(!original.tick(at(50_000)).is_sick());
    let mut restored = reboot(&original, 500_000);
    assert!(!restored.tick(at(500_000)).is_sick());
    assert!(!restored.tick(at(509_999)).is_sick());
    assert!(restored.tick(at(510_000)).is_sick());
}

#[test]
fn unattended_call_keeps_its_progress_across_power_cycles() {
    let mut original = DefaultPet::with_stats(0, 4);
    original.tick(at(0));
    assert_eq!(original.tick(at(170_000)).care_mistakes, 0);
    let mut restored = reboot(&original, 1_000_000);
    assert_eq!(restored.tick(at(1_009_999)).care_mistakes, 0);
    assert_eq!(restored.tick(at(1_010_000)).care_mistakes, 1);
}

#[test]
fn uncleaned_condition_reaches_its_remaining_one_millisecond_after_restore() {
    let data = PetArchiveData {
        poop_count: 3,
        progress: NurtureProgress::new_clamped(0, 0, 39_999, 5000),
        ..Default::default()
    };
    let mut restored = DefaultPet::from_archive(PetArchive::new_clamped(data), at(0));
    assert!(!restored.tick(at(0)).is_sick());
    assert!(restored.tick(at(1)).is_sick());
}

#[test]
fn live_sleep_and_temporary_wake_sources_are_not_restored_from_flash() {
    let mut original = DefaultPet::new();
    original.handle(PetEvent::CircadianHour(23), at(0));
    original.handle(PetEvent::Tap, at(0));
    assert!(!original.tick(at(10_000)).is_sleeping());
    let mut restored = reboot(&original, 1_000_000);
    restored.handle(PetEvent::CircadianHour(23), at(1_000_000));
    assert!(restored.tick(at(1_000_000)).is_sleeping());

    original.handle(PetEvent::FaceDown, at(10_000));
    let mut restored = reboot(&original, 0);
    assert!(!restored.tick(at(0)).is_sleeping());
    restored.handle(PetEvent::FaceDown, at(0));
    assert!(restored.tick(at(0)).is_sleeping());
}

#[test]
fn archive_constructor_protects_the_runtime_from_invalid_transfer_fields() {
    let data = PetArchiveData {
        stage: u8::MAX,
        hunger: u8::MAX,
        strength: u8::MAX,
        weight_g: 0,
        age_days: 0,
        poop_count: u8::MAX,
        energy: u8::MAX,
        ..Default::default()
    };
    let archive = PetArchive::new_clamped(data);
    let data = archive.data();
    assert_eq!(data.stage, 5);
    assert_eq!(data.hunger, 4);
    assert_eq!(data.strength, 4);
    assert_eq!(data.weight_g, 1);
    assert_eq!(data.age_days, 1);
    assert_eq!(data.poop_count, 4);
    assert_eq!(data.energy, 100);
}
