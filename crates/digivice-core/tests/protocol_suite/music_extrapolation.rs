use digivice_core::domain::{MusicMood, MusicScene, MusicSection};
use digivice_core::pet::{DefaultPet, PetAction, PetBehavior, PetEvent, SleepState};
use embassy_time::{Duration, Instant};

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

#[test]
fn beat_phase_extrapolates_smoothly_based_on_bpm_without_new_frames() {
    let mut pet = DefaultPet::new();
    let t0 = at(1000);

    // 120 BPM: 500ms per beat, beat_phase starts at 0
    let scene =
        MusicScene::new(1, 120, 0, 80, MusicMood::Excited, MusicSection::Chorus, t0).unwrap();
    pet.handle(PetEvent::MusicScene(scene), t0);

    // Initial frame at t0
    let f0 = pet.tick(t0);
    assert!(matches!(f0.action, PetAction::MusicDance { .. }));
    assert_eq!(f0.music_beat_pulse, 0);

    // At +125ms (quarter of 500ms beat): phase should be ~64 (256/4)
    let f_quarter = pet.tick(at(1125));
    assert_eq!(f_quarter.music_beat_pulse, 64);
    if let PetAction::MusicDance { beat_tick, .. } = f_quarter.action {
        assert_eq!(beat_tick, 64);
    } else {
        panic!("Pet should remain dancing");
    }

    // At +250ms (half of 500ms beat): phase should be ~128 (256/2)
    let f_half = pet.tick(at(1250));
    assert_eq!(f_half.music_beat_pulse, 128);

    // At +375ms (three-quarters): phase should be ~192
    let f_three_quarters = pet.tick(at(1375));
    assert_eq!(f_three_quarters.music_beat_pulse, 192);

    // At +500ms (one full beat): phase wraps to 0
    let f_full = pet.tick(at(1500));
    assert_eq!(f_full.music_beat_pulse, 0);
}

#[test]
fn music_stream_inactivity_stops_dance_and_resets_beat_pulse_after_three_seconds() {
    let mut pet = DefaultPet::new();
    let t0 = at(1000);

    let scene = MusicScene::new(1, 120, 0, 50, MusicMood::Happy, MusicSection::Verse, t0).unwrap();
    pet.handle(PetEvent::MusicScene(scene), t0);

    let f0 = pet.tick(t0);
    assert!(matches!(f0.action, PetAction::MusicDance { .. }));

    // Before 3 seconds: pet is still dancing
    let f_active = pet.tick(at(1000 + 2950));
    assert!(matches!(f_active.action, PetAction::MusicDance { .. }));
    assert!(f_active.music_beat_pulse > 0);

    // At and after 3 seconds without new frames: music expires
    let f_expired = pet.tick(at(1000 + 3000));
    assert_eq!(f_expired.music_beat_pulse, 0);
    assert_eq!(f_expired.action, PetAction::IdleBreathe);
}

#[test]
fn sleep_immediately_mutes_beat_pulse_and_takes_priority() {
    let mut pet = DefaultPet::new();
    let t0 = at(1000);

    let scene =
        MusicScene::new(1, 140, 50, 90, MusicMood::Excited, MusicSection::Chorus, t0).unwrap();
    pet.handle(PetEvent::MusicScene(scene), t0);

    let f = pet.tick(t0);
    assert_eq!(f.music_beat_pulse, 50);

    // FaceDown puts pet to sleep
    pet.handle(PetEvent::FaceDown, at(1050));
    let f_sleep = pet.tick(at(1050));
    assert_eq!(f_sleep.sleep_state, SleepState::Asleep);
    assert_eq!(f_sleep.music_beat_pulse, 0);
    assert!(matches!(f_sleep.action, PetAction::SleepingZzz { .. }));
}

#[test]
fn zero_bpm_produces_zero_phase() {
    let t0 = at(1000);
    let scene = MusicScene::new(1, 0, 100, 50, MusicMood::Calm, MusicSection::Verse, t0).unwrap();
    assert_eq!(scene.current_beat_phase(t0), 0);
    assert_eq!(scene.current_beat_phase(t0 + Duration::from_millis(500)), 0);
}
