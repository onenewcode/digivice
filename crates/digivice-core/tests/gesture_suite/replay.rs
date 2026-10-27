use digivice_core::domain::ImuSample;
use digivice_core::gesture::{Attitude, GestureDetector, GestureEvent};
use embassy_time::Instant;

fn sample() -> ImuSample {
    ImuSample {
        az_mg: 1000,
        ..ImuSample::default()
    }
}

#[test]
fn tilt_debounce_uses_injected_time() {
    let mut detector = GestureDetector::new_at(Instant::from_millis(10_000));
    let attitude = Attitude {
        roll_deg: 25.0,
        gz_filt_mg: 1000.0,
        ..Attitude::default()
    };
    assert_eq!(
        detector.process_imu(&sample(), &attitude, Instant::from_millis(10_000)),
        None
    );
    assert_eq!(
        detector.process_imu(&sample(), &attitude, Instant::from_millis(10_059)),
        None
    );
    assert_eq!(
        detector.process_imu(&sample(), &attitude, Instant::from_millis(10_060)),
        Some(GestureEvent::TiltRight)
    );
}

#[test]
fn face_down_debounce_uses_injected_time() {
    let mut detector = GestureDetector::new_at(Instant::from_millis(50_000));
    let attitude = Attitude {
        gz_filt_mg: -1000.0,
        ..Attitude::default()
    };
    for millis in [50_000, 50_199] {
        assert_eq!(
            detector.process_imu(&sample(), &attitude, Instant::from_millis(millis)),
            None
        );
    }
    assert_eq!(
        detector.process_imu(&sample(), &attitude, Instant::from_millis(50_200)),
        Some(GestureEvent::FaceDownEnter)
    );
}

#[test]
fn stationary_startup_does_not_generate_a_tap() {
    let mut detector = GestureDetector::new_at(Instant::from_millis(0));
    let attitude = Attitude {
        gz_filt_mg: 1000.0,
        ..Attitude::default()
    };
    for millis in (0..=1000).step_by(5) {
        assert_eq!(
            detector.process_imu(&sample(), &attitude, Instant::from_millis(millis)),
            None,
            "stationary startup at {millis}ms"
        );
    }
}

#[test]
fn face_down_has_priority_over_a_simultaneous_shake_reversal() {
    let mut detector = GestureDetector::new_at(Instant::from_millis(0));
    let attitude = Attitude {
        gz_filt_mg: -1000.0,
        ..Attitude::default()
    };
    for (millis, ax_mg) in [(0, 0), (100, 600)] {
        let s = ImuSample { ax_mg, ..sample() };
        assert_eq!(
            detector.process_imu(&s, &attitude, Instant::from_millis(millis)),
            None
        );
    }
    let reversal = ImuSample {
        ax_mg: -600,
        ..sample()
    };
    assert_eq!(
        detector.process_imu(&reversal, &attitude, Instant::from_millis(200)),
        Some(GestureEvent::FaceDownEnter)
    );
}

#[test]
fn suppressed_reversal_shocks_cannot_become_a_delayed_single_tap() {
    let mut detector = GestureDetector::new_at(Instant::from_millis(10_000));
    let attitude = Attitude {
        gz_filt_mg: 1000.0,
        roll_deg: 25.0,
        ..Attitude::default()
    };
    for (millis, ax_mg, az_mg) in [
        (10_000, 0, 1000),
        (10_005, 500, 1000),
        (10_105, -500, 1000),
        (10_600, 0, 1400),
        (10_605, 0, 1000),
        (10_700, 0, 1000),
        (11_030, 0, 1000),
    ] {
        let s = ImuSample {
            ax_mg,
            az_mg,
            ..sample()
        };
        let event = detector.process_imu(&s, &attitude, Instant::from_millis(millis));
        assert!(
            !matches!(
                event,
                Some(
                    GestureEvent::SingleTap
                        | GestureEvent::DoubleTap
                        | GestureEvent::TiltLeft
                        | GestureEvent::TiltRight
                )
            ),
            "unexpected interaction {event:?} at {millis}ms"
        );
    }
}

#[test]
fn isolated_tap_impulses_still_emit_single_and_double_taps() {
    let attitude = Attitude {
        gz_filt_mg: 1000.0,
        ..Attitude::default()
    };
    for (second_tap, expected) in [
        (None, GestureEvent::SingleTap),
        (Some(150), GestureEvent::DoubleTap),
    ] {
        let mut detector = GestureDetector::new_at(Instant::from_millis(0));
        assert_eq!(
            detector.process_imu(&sample(), &attitude, Instant::from_millis(0)),
            None
        );
        let mut emitted = None;
        for millis in (5..=500).step_by(5) {
            let az_mg = if millis == 50 || second_tap == Some(millis) {
                1200
            } else if millis == 55 || second_tap == Some(millis - 5) {
                800
            } else {
                1000
            };
            let s = ImuSample { az_mg, ..sample() };
            if let Some(event) = detector.process_imu(&s, &attitude, Instant::from_millis(millis)) {
                assert_eq!(emitted, None, "extra event: {event:?}");
                emitted = Some(event);
            }
        }
        assert_eq!(emitted, Some(expected));
    }
}

#[test]
fn z_axis_reversals_keep_z_as_the_principal_shake_axis() {
    let mut detector = GestureDetector::new_at(Instant::from_millis(0));
    let attitude = Attitude {
        gz_filt_mg: 1000.0,
        ..Attitude::default()
    };
    assert_eq!(
        detector.process_imu(&sample(), &attitude, Instant::from_millis(0)),
        None
    );
    let positive = ImuSample {
        az_mg: 1600,
        ..sample()
    };
    let negative = ImuSample {
        az_mg: 400,
        ..sample()
    };
    assert_eq!(
        detector.process_imu(&positive, &attitude, Instant::from_millis(100)),
        None
    );
    assert_eq!(
        detector.process_imu(&negative, &attitude, Instant::from_millis(200)),
        Some(GestureEvent::Shake)
    );
    assert_eq!(
        detector.process_imu(&positive, &attitude, Instant::from_millis(300)),
        Some(GestureEvent::Shake)
    );
}

#[test]
fn heading_events_use_the_shortest_arc_across_zero_in_both_directions() {
    let sample = sample();
    let mut detector = GestureDetector::new_at(Instant::from_millis(0));
    let attitude = |heading_deg| Attitude {
        heading_deg,
        gz_filt_mg: 1000.0,
        ..Attitude::default()
    };
    assert_eq!(
        detector.process_imu(&sample, &attitude(0.0), Instant::from_millis(0)),
        None
    );
    assert_eq!(
        detector.process_imu(&sample, &attitude(359.0), Instant::from_millis(50)),
        Some(GestureEvent::HeadingUpdate(359))
    );
    assert_eq!(
        detector.process_imu(&sample, &attitude(0.0), Instant::from_millis(100)),
        Some(GestureEvent::HeadingUpdate(0))
    );

    let mut reverse = GestureDetector::new_at(Instant::from_millis(0));
    assert_eq!(
        reverse.process_imu(&sample, &attitude(0.0), Instant::from_millis(0)),
        None
    );
    assert_eq!(
        reverse.process_imu(&sample, &attitude(1.0), Instant::from_millis(50)),
        Some(GestureEvent::HeadingUpdate(1))
    );
    assert_eq!(
        reverse.process_imu(&sample, &attitude(359.0), Instant::from_millis(100)),
        Some(GestureEvent::HeadingUpdate(359))
    );
}
