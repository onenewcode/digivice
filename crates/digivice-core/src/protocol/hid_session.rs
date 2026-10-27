//! Per-connection HID safety policy; transport and time are supplied by the caller.

use core::future::Future;

use embassy_time::{Duration, Instant, with_timeout};

use super::hid::ConsumerControlReport;
use crate::domain::HidAction;

pub const HID_NOTIFY_TIMEOUT: Duration = Duration::from_millis(250);
pub const HID_RELEASE_RETRY_DELAY: Duration = Duration::from_millis(50);
pub const HID_RELEASE_MAX_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidQueueOutcome<E> {
    Queued,
    Failed(E),
    TimedOut,
}

/// Bound the transport wait; callers must mark release pending before starting a press.
pub async fn queue_report_with_timeout<E>(
    send: impl Future<Output = Result<(), E>>,
) -> HidQueueOutcome<E> {
    match with_timeout(HID_NOTIFY_TIMEOUT, send).await {
        Ok(Ok(())) => HidQueueOutcome::Queued,
        Ok(Err(error)) => HidQueueOutcome::Failed(error),
        Err(_) => HidQueueOutcome::TimedOut,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidSubscription {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidPowerState {
    Active,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidControlCommand {
    Suspend,
    ExitSuspend,
}

impl HidControlCommand {
    pub fn decode(data: &[u8]) -> Option<Self> {
        match data {
            [0] => Some(Self::Suspend),
            [1] => Some(Self::ExitSuspend),
            _ => None,
        }
    }

    pub const fn power_state(self) -> HidPowerState {
        match self {
            Self::Suspend => HidPowerState::Suspended,
            Self::ExitSuspend => HidPowerState::Active,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidBlocked {
    Unsubscribed,
    Suspended,
    ReleasePending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidRecovery {
    RetryAt(Instant),
    Disconnect,
}

#[derive(Debug, Clone, Copy)]
struct PendingRelease {
    failures: u8,
    retry_at: Instant,
}

pub struct HidSession {
    subscription: HidSubscription,
    power: HidPowerState,
    release: Option<PendingRelease>,
}

impl HidSession {
    pub fn new(now: Instant) -> Self {
        Self {
            subscription: HidSubscription::Disabled,
            power: HidPowerState::Active,
            // Reconnecting must not replay a previous host's nonzero report.
            release: Some(PendingRelease {
                failures: 0,
                retry_at: now,
            }),
        }
    }

    pub fn set_subscription(&mut self, subscription: HidSubscription, now: Instant) {
        if self.subscription != subscription {
            self.subscription = subscription;
            self.require_release(now);
        }
    }

    pub fn set_power(&mut self, power: HidPowerState, now: Instant) {
        if self.power != power {
            self.power = power;
            self.require_release(now);
        }
    }

    pub fn begin_press(
        &mut self,
        action: HidAction,
        now: Instant,
    ) -> Result<ConsumerControlReport, HidBlocked> {
        self.check_ready()?;
        if self.release.is_some() {
            return Err(HidBlocked::ReleasePending);
        }
        // Set this BEFORE polling the transport: cancellation/error is not proof of no delivery.
        self.require_release(now);
        Ok(ConsumerControlReport::from_action(action))
    }

    pub fn next_release_at(&self) -> Option<Instant> {
        self.check_ready().ok()?;
        self.release
            .filter(|pending| pending.failures < HID_RELEASE_MAX_ATTEMPTS)
            .map(|pending| pending.retry_at)
    }

    pub fn release_due(&self, now: Instant) -> bool {
        self.next_release_at().is_some_and(|at| now >= at)
    }

    /// A local send-queue success, not an ATT/host acknowledgement.
    pub fn release_queued(&mut self) {
        if self.next_release_at().is_some() {
            self.release = None;
        }
    }

    pub fn release_failed(&mut self, now: Instant) -> HidRecovery {
        let Some(pending) = self.release.as_mut() else {
            return HidRecovery::Disconnect;
        };
        pending.failures = pending.failures.saturating_add(1);
        if pending.failures < HID_RELEASE_MAX_ATTEMPTS
            && let Some(at) = now.checked_add(HID_RELEASE_RETRY_DELAY)
        {
            pending.retry_at = at;
            return HidRecovery::RetryAt(at);
        }
        pending.failures = HID_RELEASE_MAX_ATTEMPTS;
        HidRecovery::Disconnect
    }

    fn check_ready(&self) -> Result<(), HidBlocked> {
        if self.subscription == HidSubscription::Disabled {
            Err(HidBlocked::Unsubscribed)
        } else if self.power == HidPowerState::Suspended {
            Err(HidBlocked::Suspended)
        } else {
            Ok(())
        }
    }

    fn require_release(&mut self, now: Instant) {
        self.release.get_or_insert(PendingRelease {
            failures: 0,
            retry_at: now,
        });
    }
}
