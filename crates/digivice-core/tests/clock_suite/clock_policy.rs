use digivice_core::domain::{CircadianTime, ClockSnapshot, DateTime, RtcError, RtcStatus};
use digivice_core::pet::{DefaultPet, PetBehavior, PetEvent};
use embassy_time::Instant;

fn clock(hour: u8, status: RtcStatus) -> ClockSnapshot {
    let datetime = match DateTime::new(2035, 1, 1, hour, 0, 0) {
        Ok(datetime) => datetime,
        Err(error) => panic!("invalid test calendar: {error}"),
    };
    ClockSnapshot { datetime, status }
}

fn unavailable(hour: u8) -> ClockSnapshot {
    clock(hour, RtcStatus::NeedsSetting(RtcError::NotValidated))
}

fn apply(pet: &mut DefaultPet, previous: ClockSnapshot, next: ClockSnapshot, seconds: u64) {
    if let Some(update) = next.circadian_change(previous) {
        pet.handle(update.into(), Instant::from_secs(seconds));
    }
}

#[test]
fn placeholder_hour_changes_cannot_put_the_pet_to_sleep() {
    let mut pet = DefaultPet::new();
    apply(&mut pet, unavailable(12), unavailable(22), 0);
    assert!(!pet.tick(Instant::from_secs(0)).is_sleeping());
    assert_eq!(unavailable(22).circadian_change(unavailable(12)), None);
}

#[test]
fn the_first_trusted_read_is_an_edge_even_when_the_hour_matches() {
    assert_eq!(
        clock(12, RtcStatus::HardwarePresent).circadian_change(unavailable(12)),
        Some(CircadianTime::Hour(12))
    );
    let mut pet = DefaultPet::new();
    apply(
        &mut pet,
        unavailable(22),
        clock(22, RtcStatus::HardwarePresent),
        0,
    );
    assert!(pet.tick(Instant::from_secs(0)).is_sleeping());
}

#[test]
fn anchored_software_time_still_drives_circadian_edges() {
    let mut pet = DefaultPet::new();
    let hardware = clock(21, RtcStatus::HardwarePresent);
    let fallback = clock(22, RtcStatus::FallbackSoftware(RtcError::I2cError));
    apply(&mut pet, unavailable(12), hardware, 0);
    apply(&mut pet, hardware, fallback, 1);
    assert!(pet.tick(Instant::from_secs(1)).is_sleeping());
    assert_eq!(
        clock(22, RtcStatus::HardwarePresent).circadian_change(fallback),
        None
    );
}

#[test]
fn losing_trust_removes_only_the_circadian_sleep_source() {
    let nighttime = clock(22, RtcStatus::HardwarePresent);
    for face_down in [false, true] {
        let mut pet = DefaultPet::new();
        apply(&mut pet, unavailable(12), nighttime, 0);
        if face_down {
            pet.handle(PetEvent::FaceDown, Instant::from_secs(1));
        }
        apply(&mut pet, nighttime, unavailable(22), 2);
        assert_eq!(pet.tick(Instant::from_secs(2)).is_sleeping(), face_down);
        pet.handle(PetEvent::FaceUp, Instant::from_secs(3));
        assert!(!pet.tick(Instant::from_secs(3)).is_sleeping());
    }
}

#[test]
fn duplicate_readings_and_source_recovery_do_not_replay_circadian_events() {
    let hardware = clock(23, RtcStatus::HardwarePresent);
    let fallback = clock(23, RtcStatus::FallbackSoftware(RtcError::OscillatorStopped));
    assert_eq!(hardware.circadian_change(hardware), None);
    assert_eq!(fallback.circadian_change(hardware), None);
    assert_eq!(hardware.circadian_change(fallback), None);
}
