use digivice_core::pet::{DefaultPet, PetBehavior, PetEvent, SleepState};
use embassy_time::Instant;

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

#[test]
fn face_down_remains_asleep_across_dawn_and_daytime_hour_changes() {
    let mut pet = DefaultPet::new();
    pet.handle(PetEvent::CircadianHour(6), at(0));
    pet.handle(PetEvent::FaceDown, at(100));
    for (hour, millis) in [(7, 200), (8, 300), (12, 400)] {
        pet.handle(PetEvent::CircadianHour(hour), at(millis));
        assert_eq!(pet.tick(at(millis)).sleep_state, SleepState::Asleep);
    }
    pet.handle(PetEvent::FaceUp, at(500));
    assert_eq!(pet.tick(at(500)).sleep_state, SleepState::Awake);
}

#[test]
fn picking_up_at_night_does_not_cancel_circadian_sleep() {
    let mut pet = DefaultPet::new();
    pet.handle(PetEvent::CircadianHour(23), at(0));
    pet.handle(PetEvent::FaceDown, at(100));
    pet.handle(PetEvent::FaceUp, at(200));
    assert_eq!(pet.tick(at(200)).sleep_state, SleepState::Asleep);
}

#[test]
fn night_tap_wakes_temporarily_then_returns_to_sleep() {
    let mut pet = DefaultPet::new();
    pet.handle(PetEvent::CircadianHour(23), at(0));
    pet.handle(PetEvent::Tap, at(1000));
    assert_eq!(pet.tick(at(1000)).sleep_state, SleepState::Awake);
    pet.handle(PetEvent::CircadianHour(0), at(2000));
    assert_eq!(pet.tick(at(2000)).sleep_state, SleepState::Awake);
    assert_eq!(pet.tick(at(30_999)).sleep_state, SleepState::Awake);
    assert_eq!(pet.tick(at(31_000)).sleep_state, SleepState::Asleep);
}

#[test]
fn taps_cannot_override_face_down_and_covering_cancels_a_temporary_wake() {
    let mut pet = DefaultPet::new();
    pet.handle(PetEvent::CircadianHour(23), at(0));
    pet.handle(PetEvent::Tap, at(1000));
    pet.handle(PetEvent::FaceDown, at(2000));
    pet.handle(PetEvent::Tap, at(3000));
    pet.handle(PetEvent::DoubleTap, at(4000));
    assert_eq!(pet.tick(at(4000)).sleep_state, SleepState::Asleep);
    pet.handle(PetEvent::FaceUp, at(5000));
    assert_eq!(pet.tick(at(5000)).sleep_state, SleepState::Asleep);
}
