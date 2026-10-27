use digivice_core::domain::DeviceOrientation;
use digivice_core::gesture::GestureEvent;
use digivice_core::gesture::{GestureChannel, OrientationSignal, publish_gesture};

#[test]
fn full_interaction_queue_cannot_drop_orientation() {
    let gestures = GestureChannel::new();
    let orientation = OrientationSignal::new();
    for _ in 0..16 {
        assert!(gestures.try_send(GestureEvent::SingleTap).is_ok());
    }
    // This is the former lossy path: a safety edge has no room in the queue.
    assert!(gestures.try_send(GestureEvent::FaceDownEnter).is_err());

    for (event, expected) in [
        (GestureEvent::FaceDownEnter, DeviceOrientation::FaceDown),
        (GestureEvent::FaceDownExit, DeviceOrientation::FaceUp),
    ] {
        assert!(publish_gesture(event, &gestures, &orientation).is_ok());
        assert_eq!(orientation.try_take(), Some(expected));
    }
    assert!(publish_gesture(GestureEvent::Shake, &gestures, &orientation).is_err());
    for _ in 0..16 {
        assert_eq!(gestures.try_receive(), Ok(GestureEvent::SingleTap));
    }
    assert!(gestures.try_receive().is_err());
}

#[test]
fn many_flips_coalesce_to_the_final_physical_state() {
    let gestures = GestureChannel::new();
    let orientation = OrientationSignal::new();
    for _ in 0..100 {
        assert!(publish_gesture(GestureEvent::FaceDownEnter, &gestures, &orientation).is_ok());
        assert!(publish_gesture(GestureEvent::FaceDownExit, &gestures, &orientation).is_ok());
    }
    assert_eq!(orientation.try_take(), Some(DeviceOrientation::FaceUp));
    assert!(gestures.try_receive().is_err());
    assert_eq!(orientation.try_take(), None);
}
