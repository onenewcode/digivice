use digivice_core::domain::{MusicMood, MusicScene, MusicSection};
use embassy_time::Instant;

fn scene(sequence: u16, now: Instant) -> MusicScene {
    match MusicScene::new(
        sequence,
        120,
        0,
        50,
        MusicMood::Unknown,
        MusicSection::Unknown,
        now,
    ) {
        Ok(scene) => scene,
        Err(error) => panic!("valid scene rejected: {error}"),
    }
}

#[test]
fn unknown_mood_does_not_disable_sequence_ordering() {
    let received = Instant::from_millis(1_000);
    let current = scene(100, received);
    assert!(!current.should_accept(&scene(99, received), Instant::from_millis(1_500)));
    assert!(!current.should_accept(&scene(100, received), Instant::from_millis(1_500)));
    assert!(current.should_accept(&scene(101, received), Instant::from_millis(1_500)));
}

#[test]
fn timeout_opens_a_new_session_even_when_the_first_new_sequence_is_lower() {
    let current = scene(500, Instant::from_millis(0));
    assert!(current.should_accept(
        &scene(1, Instant::from_millis(3_000)),
        Instant::from_millis(3_001)
    ));
    assert!(!current.should_accept(
        &scene(1, Instant::from_millis(0)),
        Instant::from_millis(2_999)
    ));
}

#[test]
fn wrapping_sequence_numbers_remain_newer_inside_one_active_session() {
    let current = scene(u16::MAX, Instant::from_millis(0));
    assert!(current.should_accept(&scene(0, Instant::from_millis(1)), Instant::from_millis(2)));
    assert!(!current.should_accept(
        &scene(u16::MAX - 1, Instant::from_millis(1)),
        Instant::from_millis(2)
    ));
}
