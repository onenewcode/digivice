use core::cell::Cell;
use core::future::{Future, ready};
use core::pin::{Pin, pin};
use core::task::{Context, Poll, Waker};

use digivice_core::domain::HidAction;
use digivice_core::protocol::{
    HidBlocked, HidQueueOutcome, HidRecovery, HidSession, HidSubscription,
    queue_report_with_timeout,
};
use embassy_time::{Duration, Instant, MockDriver};

struct StalledSend<'a>(&'a Cell<u32>);

impl Future for StalledSend<'_> {
    type Output = Result<(), u8>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

impl Drop for StalledSend<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn actual_notify_budget_distinguishes_results_and_preserves_release_after_timeout_or_cancellation()
{
    // Only this test uses the global driver in this test executable.
    let driver = MockDriver::get();
    driver.reset();
    let mut cx = Context::from_waker(Waker::noop());
    let mut session = HidSession::new(Instant::now());
    session.set_subscription(HidSubscription::Enabled, Instant::now());
    session.release_queued();
    assert!(
        session
            .begin_press(HidAction::PlayPause, Instant::now())
            .is_ok()
    );
    assert_eq!(
        pin!(queue_report_with_timeout(ready(Ok::<(), u8>(())))).poll(&mut cx),
        Poll::Ready(HidQueueOutcome::Queued)
    );
    assert_eq!(
        pin!(queue_report_with_timeout(ready(Err::<(), u8>(7)))).poll(&mut cx),
        Poll::Ready(HidQueueOutcome::Failed(7))
    );

    let dropped = Cell::new(0);
    {
        let mut release = pin!(queue_report_with_timeout(StalledSend(&dropped)));
        assert!(release.as_mut().poll(&mut cx).is_pending());
        driver.advance(Duration::from_millis(249));
        assert!(release.as_mut().poll(&mut cx).is_pending());
        driver.advance(Duration::from_millis(1));
        assert_eq!(
            release.as_mut().poll(&mut cx),
            Poll::Ready(HidQueueOutcome::TimedOut)
        );
    }
    assert_eq!(dropped.get(), 1, "timed-out transport must be dropped");
    assert_eq!(
        session.release_failed(Instant::now()),
        HidRecovery::RetryAt(Instant::from_millis(300))
    );
    assert_eq!(
        session.begin_press(HidAction::VolumeUp, Instant::now()),
        Err(HidBlocked::ReleasePending)
    );
    driver.advance(Duration::from_millis(49));
    assert!(!session.release_due(Instant::now()));
    driver.advance(Duration::from_millis(1));
    assert!(session.release_due(Instant::now()));
    assert_eq!(
        pin!(queue_report_with_timeout(ready(Ok::<(), u8>(())))).poll(&mut cx),
        Poll::Ready(HidQueueOutcome::Queued)
    );
    session.release_queued();

    assert!(
        session
            .begin_press(HidAction::VolumeUp, Instant::now())
            .is_ok()
    );
    {
        let mut press = pin!(queue_report_with_timeout(StalledSend(&dropped)));
        assert!(press.as_mut().poll(&mut cx).is_pending());
        // The caller is cancelled before its timeout; no completion is delivered.
    }
    assert_eq!(dropped.get(), 2);
    assert!(session.release_due(Instant::now()));
    assert_eq!(
        session.begin_press(HidAction::VolumeDown, Instant::now()),
        Err(HidBlocked::ReleasePending)
    );
}
