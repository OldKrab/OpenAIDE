//! Shared synchronization vocabulary for this crate's tests. Every duration a
//! test needs is named here, so a test never sizes a wait to how long an
//! operation usually takes. `scripts/check-test-timing.mjs` enforces that.

use std::time::{Duration, Instant};

/// Deadlock watchdog for a wait on an outcome the test has already made
/// inevitable. It only bounds a hung test and never establishes ordering, so
/// it is far longer than a loaded runner needs.
pub(crate) const WATCHDOG: Duration = Duration::from_secs(30);

/// A product expiry the test waits out. Pair it with `NEVER` on every
/// competing expiry so which one applies is observed as an outcome.
pub(crate) const EXPIRES: Duration = Duration::from_millis(100);

/// A product expiry that must stay out of the test's way.
pub(crate) const NEVER: Duration = Duration::from_secs(60 * 60);

/// How long an absence is observed. Load can only shorten what this window
/// sees, so it may strengthen a test and never decides whether one passes:
/// the state that excludes the event is held by a gate, not by this window.
pub(crate) const ABSENCE_WINDOW: Duration = Duration::from_millis(50);

const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Polls `observe` until it yields a value, bounded by the watchdog.
#[track_caller]
pub(crate) fn wait_for<T>(outcome: &str, mut observe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WATCHDOG;
    loop {
        if let Some(value) = observe() {
            return value;
        }
        assert!(Instant::now() < deadline, "watchdog expired: {outcome}");
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Polls `condition` until it holds, bounded by the watchdog.
#[track_caller]
pub(crate) fn wait_until(outcome: &str, mut condition: impl FnMut() -> bool) {
    wait_for(outcome, || condition().then_some(()));
}

/// Holds a fixture at a gate while `held` stays true. The test opens the gate
/// by changing the state `held` reads. The hold is unbounded: the test's own
/// waits carry the watchdog, and a fixture outliving its test must stay quiet.
pub(crate) fn hold_while(mut held: impl FnMut() -> bool) {
    while held() {
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Observes for one absence window. Call it only while a gate holds the state
/// that excludes the event, then assert the absence.
pub(crate) fn observe_absence() {
    std::thread::sleep(ABSENCE_WINDOW);
}

/// Lets a product expiry of `expiry` pass and then some. Call it only while a
/// gate holds the state under test, so the expiry is known not to apply.
pub(crate) fn outlast(expiry: Duration) {
    std::thread::sleep(expiry * 3);
}

/// Async form of `wait_for` for a test body that owns the runtime thread.
pub(crate) async fn wait_for_async<T>(outcome: &str, mut observe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WATCHDOG;
    loop {
        if let Some(value) = observe() {
            return value;
        }
        assert!(Instant::now() < deadline, "watchdog expired: {outcome}");
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
