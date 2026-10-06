# Testing guide

## Choose the evidence

Test user-observable behavior at a real boundary. Assertions must demonstrate the behavior independently of implementation text, CSS declarations, selectors, token values, or pixel constants.

Find check commands in the repository scripts and tool configuration. Run the narrowest check that can falsify the change; broaden coverage when a shared contract changes. Verification is complete when the relevant checks pass and any unverified behavior is identified at handoff.

## Regression tests

For a production behavior bug:

1. Reproduce the reported behavior at the closest real boundary. The test must fail for that behavior before the fix.
2. Implement the fix and rerun the test. Completion requires the same test to pass.

For ACP behavior, exercise chunks, updates, and replayed history through the boundary so the test retains protocol semantics.

## Concurrent tests

Establish ordering with barriers, channels, events, or latches. Await or join workers to collect results. Each required ordering relationship must have an explicit synchronization point, independent of scheduling speed.

For a flaky ordering test, replace timing assumptions with synchronization before adjusting deadlines. A generous timeout may serve as a deadlock watchdog; ordering must remain established by the synchronization primitives.

When elapsed time is itself the contract, assert that contract, preferably using a controlled clock.

## Waits and timeouts

A test passes or fails for the same reason on an idle workstation and on a saturated runner. Every duration in a test is one of three kinds; decide which before writing it.

- **Watchdog.** A wait for an outcome the test has already made inevitable only bounds a hang. Use the suite's shared budget: `crate::test_sync::WATCHDOG` in App Server unit tests, the configured Vitest timeout in Frontend, and the default `expect` polling in Playwright. Never size such a wait to how long the operation usually takes, and never give one test a private, shorter budget.
- **Absence window.** Proving that something does *not* happen needs a bounded window, which is the only place a short duration belongs. First synchronize on the point after which the event would have occurred, then keep the window short and say what it excludes.
- **Elapsed-time contract.** When a latency or deadline is the behavior, assert it explicitly and prefer a controlled clock.

Wait for the outcome that proves the behavior, not for a state that is also true earlier. An accepted Send is idle with messages before its Turn starts; "the reply arrived" identifies the finished Turn and "idle with messages" does not. Never replace such a wait with a sleep.

Drive asynchronous input once, then wait for its effect. A wheel, key, or pointer gesture is applied after the call returns, so a retry block that repeats the gesture compounds it. Retry only the read: poll the observation, and keep non-idempotent actions outside `toPass` and polling callbacks. Follow-up input must not depend on the exact value the first gesture settled at.

Input that must land inside a specific frame or between two internal steps cannot be timed from outside the page. Place it from inside, at an observable hook that orders it relative to the behavior under test, and state that ordering in the test.

Keep expensive resources out of unit suites where a lighter boundary proves the behavior. A unit test that launches a browser or runs a production build competes with every parallel worker, so it relies on the suite watchdog and never on its own budget.

A test that fails intermittently is reporting a defect. Reproduce it under load, for example by repeating it across more workers than cores, and determine whether the test or the product is wrong before changing either. Retries, longer sleeps, and looser assertions hide the answer. When the product is wrong, add a deterministic regression test for that behavior as described above.

## Visual verification

For visual-only changes, verify the affected interaction in the browser. Use relevant wide and narrow viewports; shared UI requires both. Shell composition also requires the default and override paths. Completion requires observing the interaction in every applicable viewport and composition path.

## Rust test placement

Put test bodies in dedicated files. Use integration tests by default; private unit tests use the adjacent `<module>_tests.rs` convention. Shared integration helpers live in `tests/common/mod.rs`.
