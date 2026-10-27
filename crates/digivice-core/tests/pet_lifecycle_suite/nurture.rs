use digivice_core::pet::{
    AttentionState, DefaultPet, HealthStatus, PetBehavior, PetEvent, PetFrame, SleepState,
};
use embassy_time::Instant;

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

fn physiology(frame: PetFrame) -> (u8, u8, HealthStatus, AttentionState, u8) {
    (
        frame.hunger,
        frame.poop_count,
        frame.health,
        frame.attention,
        frame.care_mistakes,
    )
}

#[test]
fn unattended_call_is_exactly_three_real_minutes() {
    let mut pet = DefaultPet::with_stats(0, 4);
    pet.tick(at(0));
    assert_eq!(pet.tick(at(179_999)).care_mistakes, 0);
    assert_eq!(pet.tick(at(180_000)).care_mistakes, 1);
    assert_eq!(pet.tick(at(360_000)).care_mistakes, 2);
}

#[test]
fn starvation_sickness_uses_sixty_awake_seconds() {
    let mut pet = DefaultPet::with_stats(0, 4);
    pet.tick(at(0));
    assert_eq!(pet.tick(at(59_999)).health, HealthStatus::Healthy);
    assert_eq!(pet.tick(at(60_000)).health, HealthStatus::Sick);
}

#[test]
fn uncleaned_sickness_uses_forty_seconds_after_the_third_poop() {
    let mut pet = DefaultPet::new();
    pet.tick(at(0));
    for millis in [90_000, 120_000, 150_000] {
        pet.handle(PetEvent::DoubleTap, at(millis));
    }
    assert_eq!(pet.tick(at(189_999)).health, HealthStatus::Healthy);
    assert_eq!(pet.tick(at(190_000)).health, HealthStatus::Sick);
}

#[test]
fn physiology_is_independent_of_20_30_40_hz_and_long_stalls() {
    let mut jumped = DefaultPet::new();
    jumped.tick(at(0));
    let expected = physiology(jumped.tick(at(600_000)));
    assert_eq!(
        expected,
        (0, 4, HealthStatus::Sick, AttentionState::Calling, 2)
    );

    for hz in [20, 30, 40] {
        let mut pet = DefaultPet::new();
        for frame in 0..=600 * hz {
            pet.tick(Instant::from_micros(frame * 1_000_000 / hz));
        }
        assert_eq!(physiology(pet.tick(at(600_000))), expected, "{hz}Hz");
    }

    let mut irregular = DefaultPet::new();
    for millis in [0, 1, 29_999, 30_000, 89_999, 90_000, 119_000, 600_000] {
        irregular.tick(at(millis));
    }
    assert_eq!(physiology(irregular.tick(at(600_000))), expected);
}

#[test]
fn sleep_pauses_nurture_without_erasing_accumulated_care_time() {
    let mut pet = DefaultPet::with_stats(0, 4);
    pet.tick(at(0));
    pet.tick(at(100_000));
    pet.handle(PetEvent::FaceDown, at(100_000));
    let before = physiology(pet.tick(at(100_000)));
    assert_eq!(physiology(pet.tick(at(500_000))), before);
    pet.handle(PetEvent::FaceUp, at(500_000));
    assert_eq!(pet.tick(at(579_999)).care_mistakes, 0);
    assert_eq!(pet.tick(at(580_000)).care_mistakes, 1);
}

#[test]
fn feeding_after_a_stall_accounts_for_time_before_the_feed() {
    let mut pet = DefaultPet::new();
    pet.tick(at(0));
    pet.handle(PetEvent::DoubleTap, at(90_000));
    assert_eq!(pet.tick(at(90_000)).hunger, 2);
    assert_eq!(pet.tick(at(120_000)).hunger, 1);
}

#[test]
fn a_long_stall_does_not_extend_the_nighttime_awake_window() {
    let mut pet = DefaultPet::with_stats(0, 4);
    pet.handle(PetEvent::CircadianHour(23), at(0));
    pet.handle(PetEvent::Tap, at(1000));
    let frame = pet.tick(at(601_000));
    assert_eq!(frame.sleep_state, SleepState::Asleep);
    assert_eq!(frame.poop_count, 1);
    assert_eq!(frame.care_mistakes, 0);
}

#[test]
fn stale_timestamps_do_not_double_count_nurture_time() {
    let mut pet = DefaultPet::new();
    pet.tick(at(0));
    pet.tick(at(30_000));
    pet.tick(at(10_000));
    assert_eq!(pet.tick(at(60_000)).hunger, 2);
}

#[test]
fn huge_time_jumps_are_bounded_and_saturate_without_overflow() {
    let mut pet = DefaultPet::new();
    pet.tick(Instant::from_ticks(0));
    let frame = pet.tick(Instant::from_ticks(u64::MAX));
    assert_eq!(frame.hunger, 0);
    assert_eq!(frame.poop_count, 4);
    assert_eq!(frame.health, HealthStatus::Sick);
    assert_eq!(frame.care_mistakes, u8::MAX);
}
