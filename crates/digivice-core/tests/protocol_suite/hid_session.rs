use digivice_core::domain::HidAction;
use digivice_core::protocol::{
    HID_NOTIFY_TIMEOUT, HID_RELEASE_MAX_ATTEMPTS, HID_RELEASE_RETRY_DELAY, HidBlocked,
    HidControlCommand, HidPowerState, HidRecovery, HidSession, HidSubscription,
};
use embassy_time::Instant;

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

fn ready() -> HidSession {
    let mut session = HidSession::new(at(0));
    session.set_subscription(HidSubscription::Enabled, at(0));
    assert!(session.release_due(at(0)));
    session.release_queued();
    session
}

#[test]
fn subscription_requires_a_neutral_report_before_the_first_press() {
    let mut session = HidSession::new(at(0));
    assert_eq!(
        session.begin_press(HidAction::PlayPause, at(1)),
        Err(HidBlocked::Unsubscribed)
    );
    assert_eq!(session.next_release_at(), None);
    session.set_subscription(HidSubscription::Enabled, at(2));
    assert_eq!(
        session.begin_press(HidAction::PlayPause, at(2)),
        Err(HidBlocked::ReleasePending)
    );
    session.release_queued();
    assert_eq!(
        session
            .begin_press(HidAction::PlayPause, at(3))
            .map(|report| report.bytes()),
        Ok([0xCD, 0])
    );
}

#[test]
fn every_press_attempt_requires_release_even_if_the_transport_is_cancelled_or_fails() {
    for action in [
        HidAction::PlayPause,
        HidAction::PreviousTrack,
        HidAction::NextTrack,
        HidAction::VolumeUp,
        HidAction::VolumeDown,
        HidAction::Mute,
    ] {
        let mut session = ready();
        assert!(session.begin_press(action, at(1)).is_ok());
        // No send completion is delivered: release still takes priority over every new press.
        assert!(session.release_due(at(1)));
        assert_eq!(
            session.begin_press(action, at(2)),
            Err(HidBlocked::ReleasePending)
        );
        session.release_queued();
        assert!(session.begin_press(action, at(3)).is_ok());
    }
}

#[test]
fn release_failure_blocks_new_presses_until_a_delayed_retry_succeeds() {
    let mut session = ready();
    assert!(session.begin_press(HidAction::VolumeUp, at(1)).is_ok());
    assert_eq!(session.release_failed(at(10)), HidRecovery::RetryAt(at(60)));
    assert!(!session.release_due(at(59)));
    assert!(session.release_due(at(60)));
    assert_eq!(
        session.begin_press(HidAction::VolumeDown, at(60)),
        Err(HidBlocked::ReleasePending)
    );
    session.release_queued();
    assert!(session.begin_press(HidAction::VolumeDown, at(61)).is_ok());
}

#[test]
fn repeated_release_failure_terminates_instead_of_retrying_or_pressing_forever() {
    let mut session = ready();
    assert!(session.begin_press(HidAction::Mute, at(0)).is_ok());
    assert_eq!(HID_RELEASE_MAX_ATTEMPTS, 3);
    assert_eq!(HID_NOTIFY_TIMEOUT.as_millis(), 250);
    assert_eq!(HID_RELEASE_RETRY_DELAY.as_millis(), 50);
    for (time, expected) in [
        (250, HidRecovery::RetryAt(at(300))),
        (550, HidRecovery::RetryAt(at(600))),
        (850, HidRecovery::Disconnect),
    ] {
        assert_eq!(session.release_failed(at(time)), expected);
    }
    assert_eq!(session.next_release_at(), None);
    session.release_queued(); // A stale success cannot revive a terminated recovery.
    assert_eq!(
        session.begin_press(HidAction::Mute, at(1000)),
        Err(HidBlocked::ReleasePending)
    );
}

#[test]
fn unsubscribing_retains_release_and_cannot_turn_a_noop_notify_into_success() {
    let mut session = ready();
    assert!(session.begin_press(HidAction::NextTrack, at(0)).is_ok());
    session.set_subscription(HidSubscription::Disabled, at(1));
    assert_eq!(session.next_release_at(), None);
    session.release_queued();
    session.set_subscription(HidSubscription::Enabled, at(2));
    assert!(session.release_due(at(2)));
    assert_eq!(
        session.begin_press(HidAction::NextTrack, at(2)),
        Err(HidBlocked::ReleasePending)
    );
    session.release_queued();
    assert!(session.begin_press(HidAction::NextTrack, at(3)).is_ok());
}

#[test]
fn suspend_blocks_reports_and_exit_requires_neutral_before_fresh_actions() {
    let mut session = ready();
    assert!(session.begin_press(HidAction::PlayPause, at(0)).is_ok());
    session.set_power(HidPowerState::Suspended, at(1));
    assert_eq!(session.next_release_at(), None);
    assert_eq!(
        session.begin_press(HidAction::PlayPause, at(1)),
        Err(HidBlocked::Suspended)
    );
    session.release_queued();
    session.set_power(HidPowerState::Active, at(2));
    assert!(session.release_due(at(2)));
    session.release_queued();
    assert!(session.begin_press(HidAction::PlayPause, at(3)).is_ok());
}

#[test]
fn duplicate_observations_do_not_rearm_a_clear_session_or_reset_failure_budgets() {
    let mut session = ready();
    for time in 1..100 {
        session.set_subscription(HidSubscription::Enabled, at(time));
        session.set_power(HidPowerState::Active, at(time));
        assert_eq!(session.next_release_at(), None);
    }
    assert!(session.begin_press(HidAction::Mute, at(100)).is_ok());
    assert_eq!(
        session.release_failed(at(100)),
        HidRecovery::RetryAt(at(150))
    );
    session.set_power(HidPowerState::Suspended, at(101));
    session.set_subscription(HidSubscription::Disabled, at(101));
    session.set_power(HidPowerState::Active, at(102));
    session.set_subscription(HidSubscription::Enabled, at(102));
    assert_eq!(session.next_release_at(), Some(at(150)));
    assert_eq!(
        session.release_failed(at(150)),
        HidRecovery::RetryAt(at(200))
    );
    assert_eq!(session.release_failed(at(200)), HidRecovery::Disconnect);
}

#[test]
fn a_new_connection_starts_neutral_and_does_not_replay_the_previous_action() {
    let mut old = ready();
    assert!(old.begin_press(HidAction::VolumeUp, at(1)).is_ok());
    let mut new = HidSession::new(at(2));
    new.set_subscription(HidSubscription::Enabled, at(2));
    assert!(new.release_due(at(2)));
    new.release_queued();
    assert_eq!(new.next_release_at(), None);
}

#[test]
fn retry_deadline_overflow_requests_disconnect_instead_of_busy_polling() {
    let mut session = ready();
    assert!(session.begin_press(HidAction::Mute, at(0)).is_ok());
    assert_eq!(
        session.release_failed(Instant::from_ticks(u64::MAX)),
        HidRecovery::Disconnect
    );
    assert_eq!(session.next_release_at(), None);
}

#[test]
fn control_point_accepts_only_one_byte_suspend_or_exit_suspend_commands() {
    assert_eq!(
        HidControlCommand::decode(&[0]),
        Some(HidControlCommand::Suspend)
    );
    assert_eq!(
        HidControlCommand::decode(&[1]),
        Some(HidControlCommand::ExitSuspend)
    );
    assert_eq!(HidControlCommand::decode(&[]), None);
    assert_eq!(HidControlCommand::decode(&[0, 1]), None);
    for reserved in 2..=u8::MAX {
        assert_eq!(HidControlCommand::decode(&[reserved]), None);
    }
    assert_eq!(
        HidControlCommand::Suspend.power_state(),
        HidPowerState::Suspended
    );
    assert_eq!(
        HidControlCommand::ExitSuspend.power_state(),
        HidPowerState::Active
    );
}
