# Testing guide

## Choose the evidence

Test user-observable behavior at a real boundary. Assertions must demonstrate the behavior independently of implementation text, CSS declarations, selectors, token values, or pixel constants.

Find check commands in the repository scripts and tool configuration. Run the narrowest check that can falsify the change; broaden coverage when a shared contract changes. Verification is complete when the relevant checks pass and any unverified behavior is identified at handoff.

## Regression tests

For a production behavior bug:

1. Reproduce the reported behavior at the closest real boundary. The test must fail for that behavior before the fix.
2. Implement the fix and rerun the test. Completion requires the same test to pass.

For ACP behavior, exercise chunks, updates, and replayed history through the boundary so the test retains protocol semantics.

## Time in tests

A test passes or fails for the same reason on an idle workstation and on a saturated runner. Order every step with a **gate**: a barrier, channel, latch, or release file the test opens once it has observed the state it needs. A fixture holds at its gate until the test releases it, so each ordering relationship has one explicit synchronization point.

Every duration a test still names is one of these kinds. `npm run check:test-timing` reports any other use of time in test code; a `timing: <kind> — reason` comment on the line or the line above declares the kind, and `timing-file:` in the file header declares it for a whole file.

| Kind | What it is | How to write it |
| --- | --- | --- |
| watchdog | Bounds a hang while waiting for an outcome the test has made inevitable. It expires only when the test is already failing. | Use the suite's shared 30-second watchdog, unmarked. |
| `expiry` | A product timeout the test waits out. | Hold the competing work at a gate so the expiry is the only thing that can happen, and set every other expiry to never. |
| `absence` | A window in which something must stay absent. | Hold the state that excludes the event at a gate, observe one window, then assert. Load shortens what the window sees and leaves the result unchanged. |
| `contract` | Elapsed time is the behavior under test. | Assert the contract on a `mocked` clock where the code allows one; otherwise keep two orders of magnitude between the durations involved and say what they are. |
| `mocked` | Time on a controlled clock: paused tokio time, Vitest fake timers, `node:test` mock timers. | Advance the clock from the test. |
| `poll` | The interval of a loop that re-reads an observation or a gate. | Bound the loop with the watchdog. |
| `data` | A duration passed or compared as a value and never waited on. | Mark it. |

Each suite has one home for its watchdog and gates:

- App Server Rust tests: `openaide-rs/app-server/src/test_sync.rs` names every duration and wait. Embedded Python fixtures hold at release files.
- Playwright smoke tests: the configured `expect` timeout is the watchdog. `harness.hold(name)` and `harness.release(name)` drive the gates an Agent fixture awaits with `released(name)`.
- Node and Vitest tests: await the event or promise that reports the outcome; drive timers with the mocked clock.

Wait for the outcome that proves the behavior. An accepted Send is idle with messages before its Turn starts; "the reply arrived" identifies the finished Turn.

Drive asynchronous input once, then poll the observation. A wheel, key, or pointer gesture lands after the call returns, so keep it outside `toPass` and polling callbacks, and write follow-up input that holds for any value the first gesture settles at.

Place input that must land inside a specific frame from inside the page, at an observable hook that orders it relative to the behavior under test, and state that ordering in the test.

Prove behavior at the lightest boundary that shows it. A test that launches a browser, a process, or a production build runs under the watchdog.

## Ports

The process that serves a port binds it: start it on port 0 and read the bound port from its output. For an address that must refuse connections, keep the port owned by a socket that is not listening for as long as the test needs the refusal.

## Flaky tests

An intermittent failure reports a defect in the test or in the product. Reproduce it under load by repeating the test across more workers than cores. Find the two events that raced and which one the test assumed came first, then decide which side is wrong:

- The test assumed an order: add the gate that establishes it.
- The product allowed an order it should exclude: fix the product and add a regression test that holds that order at a gate.

The fix is complete when the repeated run under load passes and the test names no duration outside the kinds above.

## Visual verification

For visual-only changes, verify the affected interaction in the browser. Use relevant wide and narrow viewports; shared UI requires both. Shell composition also requires the default and override paths. Completion requires observing the interaction in every applicable viewport and composition path.

## Rust test placement

Put test bodies in dedicated files. Use integration tests by default; private unit tests use the adjacent `<module>_tests.rs` convention. Shared integration helpers live in `tests/common/mod.rs`.
