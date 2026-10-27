use digivice_core::protocol::presence::{BlePresenceTracker, TargetBinding};
use digivice_core::protocol::{PresenceState, RadioReport, RadioStatus, TargetVisibility};
use embassy_time::Instant;

const TARGET: [u8; 6] = [1, 2, 3, 4, 5, 6];
const OTHER: [u8; 6] = [9, 8, 7, 6, 5, 4];

#[test]
fn tracker_starts_with_configured_status_and_is_near_respects_operational() {
    let binding = TargetBinding::new(7).unwrap();
    let tracker = BlePresenceTracker::new_with_away_threshold_and_status(
        Some(binding),
        -75,
        RadioStatus::ControllerUnavailable,
    );
    assert_eq!(tracker.radio_status, RadioStatus::ControllerUnavailable);
    assert!(!tracker.radio_status.is_operational());
    assert!(tracker.radio_status.is_fault());
    assert!(!tracker.is_near());
}

#[test]
fn failing_status_immediately_clears_active_near_presence_and_blips() {
    let binding = TargetBinding::new(7).unwrap();
    let mut tracker = BlePresenceTracker::new(Some(binding));

    // 1. Receive advertisement while Active -> becomes Near
    tracker.process_advertisement(&TARGET, -10, Instant::from_millis(100));
    assert_eq!(tracker.presence, PresenceState::Near);
    assert!(tracker.is_near());
    assert_eq!(tracker.target_visibility, TargetVisibility::Visible);
    assert!(!tracker.blips.is_empty());

    // 2. Radio runner exits / fails -> transitions to fault status
    tracker.set_radio_status(RadioStatus::RunnerExited);
    assert_eq!(tracker.radio_status, RadioStatus::RunnerExited);
    assert!(!tracker.radio_status.is_operational());
    assert!(tracker.radio_status.is_fault());
    assert_eq!(tracker.presence, PresenceState::Away);
    assert!(!tracker.is_near());
    assert_eq!(tracker.target_visibility, TargetVisibility::Hidden);
    assert_eq!(tracker.smoothed_target_rssi, -85);
    assert!(tracker.blips.is_empty());
}

#[test]
fn advertisements_ignored_when_radio_not_operational() {
    let binding = TargetBinding::new(0x1234).unwrap();
    let mut tracker = BlePresenceTracker::new_with_away_threshold_and_status(
        Some(binding),
        -75,
        RadioStatus::ControllerUnavailable,
    );

    tracker.process_advertisement(&TARGET, -30, Instant::from_millis(50));
    tracker.process_advertisement(&OTHER, -20, Instant::from_millis(60));

    assert_eq!(tracker.presence, PresenceState::Away);
    assert_eq!(tracker.target_visibility, TargetVisibility::Hidden);
    assert!(tracker.blips.is_empty());
    assert_eq!(
        tracker.update_watchdog(Instant::from_millis(1000)),
        PresenceState::Away
    );
}

#[test]
fn radio_report_status_query_invariants() {
    let report_active_near = RadioReport {
        status: RadioStatus::Active,
        target_bound: true.into(),
        target_seen: TargetVisibility::Visible,
        presence: PresenceState::Near,
        ble_rssi: -50,
        ble_blips: Default::default(),
    };
    assert!(report_active_near.is_operational());
    assert!(report_active_near.is_near());

    let report_failed_near = RadioReport {
        status: RadioStatus::RunnerExited,
        target_bound: true.into(),
        target_seen: TargetVisibility::Visible,
        presence: PresenceState::Near, // Even if raw presence field were Near
        ble_rssi: -50,
        ble_blips: Default::default(),
    };
    assert!(!report_failed_near.is_operational());
    assert!(
        !report_failed_near.is_near(),
        "Non-operational radio report must never report is_near"
    );
}
