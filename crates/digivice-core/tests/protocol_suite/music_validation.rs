use digivice_core::domain::{MusicMood, MusicScene, MusicSceneError, MusicSection};
use embassy_time::Instant;

fn make(bpm: u16) -> Result<MusicScene, MusicSceneError> {
    MusicScene::new(
        1,
        bpm,
        0,
        50,
        MusicMood::Unknown,
        MusicSection::Unknown,
        Instant::from_millis(0),
    )
}

#[test]
fn constructor_and_gatt_decoder_share_the_same_bpm_boundaries() {
    for bpm in [0, 40, 240] {
        assert!(make(bpm).is_ok());
        let mut frame = [0u8; 10];
        frame[0] = 1;
        frame[3..5].copy_from_slice(&bpm.to_le_bytes());
        assert!(MusicScene::from_frame(&frame, Instant::from_millis(0)).is_ok());
    }
    for bpm in [1, 39, 241, u16::MAX] {
        assert_eq!(make(bpm), Err(MusicSceneError::InvalidBpm));
        let mut frame = [0u8; 10];
        frame[0] = 1;
        frame[3..5].copy_from_slice(&bpm.to_le_bytes());
        assert_eq!(
            MusicScene::from_frame(&frame, Instant::from_millis(0)),
            Err(MusicSceneError::InvalidBpm)
        );
    }
}
