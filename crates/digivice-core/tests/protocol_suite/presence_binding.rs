use digivice_core::protocol::presence::{BlePresenceTracker, TargetBinding};
use digivice_core::protocol::{PresenceState, RadioStatus, TargetVisibility};
use embassy_time::{Duration, Instant};

#[test]
fn target_binding_computes_consistent_xor_hash_and_matching() {
    let mac = [1, 2, 3, 4, 5, 6];
    // low byte: 1 ^ 3 ^ 5 = 7
    // high byte: 2 ^ 4 ^ 6 = 0
    // total: 7 | (0 << 8) = 7
    let binding = TargetBinding::from_addr(&mac).expect("Non-zero hash must succeed");
    assert_eq!(binding.hash(), 7);
    assert_eq!(TargetBinding::hash_addr(&mac), 7);
    assert!(binding.matches_addr(&mac));

    // Zero hash is rejected
    let zero_mac = [0, 0, 0, 0, 0, 0];
    assert!(TargetBinding::from_addr(&zero_mac).is_none());
    assert_eq!(TargetBinding::hash_addr(&zero_mac), 0);
}

#[test]
fn xor_folding_collision_demonstrates_opportunistic_heuristic_nature() {
    // Two completely different MAC addresses producing the exact same 16-bit folded hash
    let mac_a = [1, 2, 3, 4, 5, 6];
    let mac_b = [7, 0, 0, 0, 0, 0]; // 7 ^ 0 ^ 0 = 7, 0 ^ 0 ^ 0 = 0 -> hash is 7!

    assert_ne!(mac_a, mac_b);
    assert_eq!(
        TargetBinding::hash_addr(&mac_a),
        TargetBinding::hash_addr(&mac_b)
    );

    let binding_a = TargetBinding::from_addr(&mac_a).unwrap();
    assert!(
        binding_a.matches_addr(&mac_b),
        "Demonstrates 16-bit hash collision: cannot distinguish mac_a from mac_b without cryptographic IRK"
    );
}

#[test]
fn mobile_rpa_rotation_drops_presence_to_away_after_timeout() {
    let initial_rpa = [0x4A, 0x12, 0x34, 0x56, 0x78, 0x9A];
    let binding = TargetBinding::from_addr(&initial_rpa).unwrap();

    let mut tracker = BlePresenceTracker::new_with_away_threshold_and_status(
        Some(binding),
        -78,
        RadioStatus::Active,
    );

    // Initial packets arrive: device is detected as Near
    tracker.process_advertisement(&initial_rpa, -10, Instant::from_millis(100));
    assert_eq!(tracker.presence, PresenceState::Near);
    assert_eq!(tracker.target_visibility, TargetVisibility::Visible);

    // 15 minutes later: Smartphone rotates RPA to a new random private address
    let rotated_rpa = [0x5B, 0x99, 0x88, 0x77, 0x66, 0x55];
    assert_ne!(
        TargetBinding::hash_addr(&initial_rpa),
        TargetBinding::hash_addr(&rotated_rpa)
    );
    assert!(!binding.matches_addr(&rotated_rpa));

    // Rotated packets arrive but do not match the old hash
    tracker.process_advertisement(&rotated_rpa, -50, Instant::from_millis(200));

    // Watchdog advances past 15 seconds without seeing initial_rpa
    let timeout = tracker.update_watchdog(Instant::from_millis(100) + Duration::from_secs(16));
    assert_eq!(timeout, PresenceState::Away);
    assert_eq!(tracker.target_visibility, TargetVisibility::Hidden);
    assert_eq!(tracker.smoothed_target_rssi, -85);
}
