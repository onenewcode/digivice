use digivice_core::protocol::presence::{BlePresenceTracker, TargetBinding};
use digivice_core::protocol::{PresenceState, TargetVisibility};
use embassy_time::Instant;

const TARGET: [u8; 6] = [1, 2, 3, 4, 5, 6];

#[test]
fn target_visibility_rssi_and_blip_expire_together_after_fifteen_seconds() {
    let binding = match TargetBinding::new(7) {
        Some(binding) => binding,
        None => panic!("valid binding rejected"),
    };
    let mut tracker = BlePresenceTracker::new(Some(binding));
    tracker.process_advertisement(&TARGET, -10, Instant::from_millis(0));
    assert_eq!(tracker.presence, PresenceState::Near);
    assert_eq!(tracker.target_visibility, TargetVisibility::Visible);
    assert_eq!(tracker.blips.len(), 1);
    assert_eq!(
        tracker.update_watchdog(Instant::from_millis(15_001)),
        PresenceState::Away
    );
    assert_eq!(tracker.target_visibility, TargetVisibility::Hidden);
    assert_eq!(tracker.smoothed_target_rssi, -85);
    assert!(tracker.blips.is_empty());
}

#[test]
fn configured_away_threshold_reaches_the_presence_tracker_with_a_ten_db_hysteresis() {
    let tracker = BlePresenceTracker::new_with_away_threshold(None, -60);
    assert_eq!(tracker.rssi_away_threshold, -60);
    assert_eq!(tracker.rssi_near_threshold, -50);
}
