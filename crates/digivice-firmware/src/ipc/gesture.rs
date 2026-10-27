//! Separate authoritative orientation state from lossy interaction edges.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, TrySendError};
use embassy_sync::signal::Signal;

use crate::domain::DeviceOrientation;
use crate::gesture::GestureEvent;

pub type GestureChannel = Channel<CriticalSectionRawMutex, GestureEvent, 16>;
pub type OrientationSignal = Signal<CriticalSectionRawMutex, DeviceOrientation>;

/// Safety state overwrites stale state; only ordinary interaction edges may be dropped.
pub fn publish_gesture(
    event: GestureEvent,
    gestures: &GestureChannel,
    orientation: &OrientationSignal,
) -> Result<(), TrySendError<GestureEvent>> {
    match event {
        GestureEvent::FaceDownEnter => orientation.signal(DeviceOrientation::FaceDown),
        GestureEvent::FaceDownExit => orientation.signal(DeviceOrientation::FaceUp),
        _ => return gestures.try_send(event),
    }
    Ok(())
}
