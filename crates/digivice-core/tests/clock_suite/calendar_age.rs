use digivice_core::domain::{
    CalendarDay, ClockSnapshot, DateTime, PetArchive, PetArchiveData, RtcError, RtcStatus,
};
use digivice_core::pet::{DefaultPet, PetBehavior, PetEvent};
use digivice_core::storage::DeviceConfig;
use embassy_time::Instant;

#[test]
fn hour_rollovers_alone_cannot_invent_calendar_days() {
    let mut pet = DefaultPet::new();
    for (millis, hour) in [(0, 23), (1, 0), (2, 23), (3, 0)] {
        pet.handle(PetEvent::CircadianHour(hour), Instant::from_millis(millis));
    }
    assert_eq!(pet.tick(Instant::from_millis(3)).age_days, 1);
}

fn clock(year: u16, month: u8, day: u8, status: RtcStatus) -> ClockSnapshot {
    let datetime = match DateTime::new(year, month, day, 12, 0, 0) {
        Ok(datetime) => datetime,
        Err(error) => panic!("invalid test calendar: {error}"),
    };
    ClockSnapshot { datetime, status }
}

fn day(year: u16, month: u8, day: u8) -> CalendarDay {
    CalendarDay::from_datetime(&clock(year, month, day, RtcStatus::HardwarePresent).datetime)
}

fn archive(pet: &DefaultPet) -> PetArchive {
    match pet.archive() {
        Some(archive) => archive,
        None => panic!("default pet archive missing"),
    }
}

fn apply_clock(pet: &mut DefaultPet, clock: ClockSnapshot) {
    if let Some(day) = clock.trusted_calendar_day() {
        pet.handle(PetEvent::CalendarDay(day), Instant::from_millis(0));
    }
}

#[test]
fn the_first_trusted_day_establishes_a_baseline_without_inventing_an_age() {
    let mut pet = DefaultPet::new();
    apply_clock(&mut pet, clock(2035, 12, 31, RtcStatus::HardwarePresent));
    assert_eq!(pet.tick(Instant::from_millis(0)).age_days, 1);
    assert_eq!(archive(&pet).data().calendar_day, Some(day(2035, 12, 31)));
}

#[test]
fn sparse_updates_match_observing_every_supported_day() {
    let start = day(2020, 1, 1);
    let end = day(2099, 12, 31);
    let mut every_day = DefaultPet::new();
    for count in start.day_count()..=end.day_count() {
        let current = match CalendarDay::from_day_count(count) {
            Some(day) => day,
            None => panic!("supported day rejected"),
        };
        every_day.handle(PetEvent::CalendarDay(current), Instant::from_millis(0));
    }
    let mut sparse = DefaultPet::new();
    sparse.handle(PetEvent::CalendarDay(start), Instant::from_millis(0));
    sparse.handle(PetEvent::CalendarDay(end), Instant::from_millis(0));
    assert_eq!(archive(&sparse), archive(&every_day));
    assert_eq!(
        archive(&sparse).data().age_days,
        (end.day_count() - start.day_count() + 1) as u16
    );
}

#[test]
fn leap_days_and_year_boundaries_use_calendar_differences_not_hour_edges() {
    for (start, end, expected) in [
        (day(2036, 2, 28), day(2036, 3, 1), 3),
        (day(2035, 2, 28), day(2035, 3, 1), 2),
        (day(2035, 12, 31), day(2036, 1, 1), 2),
    ] {
        let mut pet = DefaultPet::new();
        pet.handle(PetEvent::CalendarDay(start), Instant::from_millis(0));
        pet.handle(PetEvent::CalendarDay(end), Instant::from_millis(0));
        assert_eq!(archive(&pet).data().age_days, expected);
    }
}

#[test]
fn rollback_and_duplicate_days_never_reduce_age_or_repeat_previously_counted_days() {
    let mut pet = DefaultPet::new();
    for current in [3, 1, 2, 3, 3] {
        apply_clock(
            &mut pet,
            clock(2035, 1, current, RtcStatus::HardwarePresent),
        );
        assert_eq!(archive(&pet).data().age_days, 1);
        assert_eq!(archive(&pet).data().calendar_day, Some(day(2035, 1, 3)));
    }
    apply_clock(&mut pet, clock(2035, 1, 4, RtcStatus::HardwarePresent));
    assert_eq!(archive(&pet).data().age_days, 2);
}

#[test]
fn offline_age_is_counted_once_while_physiology_remains_paused() {
    let data = PetArchiveData {
        age_days: 10,
        calendar_day: Some(day(2036, 2, 28)),
        ..Default::default()
    };
    let mut config = DeviceConfig::default();
    config.set_pet_archive(PetArchive::new_clamped(data));
    for _ in 0..3 {
        let decoded = match DeviceConfig::deserialize(&config.serialize()) {
            Ok(config) => config,
            Err(error) => panic!("stored archive rejected: {error}"),
        };
        let mut pet = DefaultPet::from_archive(decoded.pet_archive(), Instant::from_millis(0));
        apply_clock(&mut pet, clock(2036, 3, 1, RtcStatus::HardwarePresent));
        let restored = archive(&pet).data();
        assert_eq!(restored.age_days, 12);
        assert_eq!(restored.hunger, 4);
        assert_eq!(restored.poop_count, 0);
        assert_eq!(restored.care_mistakes, 0);
        assert_eq!(restored.progress, data.progress);
        config.set_pet_archive(archive(&pet));
    }
}

#[test]
fn untrusted_placeholders_cannot_create_or_advance_an_age_anchor() {
    let mut pet = DefaultPet::new();
    apply_clock(
        &mut pet,
        clock(2036, 2, 28, RtcStatus::NeedsSetting(RtcError::NotValidated)),
    );
    assert_eq!(archive(&pet).data().calendar_day, None);
    let software = RtcStatus::FallbackSoftware(RtcError::I2cError);
    apply_clock(&mut pet, clock(2036, 3, 1, software));
    apply_clock(&mut pet, clock(2036, 3, 3, software));
    assert_eq!(archive(&pet).data().age_days, 3);
    apply_clock(
        &mut pet,
        clock(
            2099,
            1,
            1,
            RtcStatus::NeedsSetting(RtcError::OscillatorStopped),
        ),
    );
    assert_eq!(archive(&pet).data().calendar_day, Some(day(2036, 3, 3)));
    apply_clock(&mut pet, clock(2036, 3, 4, RtcStatus::HardwarePresent));
    assert_eq!(archive(&pet).data().age_days, 4);
}

#[test]
fn age_saturates_without_overflow_on_large_forward_calendar_changes() {
    let data = PetArchiveData {
        age_days: u16::MAX - 1,
        calendar_day: Some(day(2020, 1, 1)),
        ..Default::default()
    };
    let mut pet = DefaultPet::from_archive(PetArchive::new_clamped(data), Instant::from_millis(0));
    apply_clock(&mut pet, clock(2099, 12, 31, RtcStatus::HardwarePresent));
    assert_eq!(archive(&pet).data().age_days, u16::MAX);
}
