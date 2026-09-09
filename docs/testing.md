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

## Visual verification

For visual-only changes, verify the affected interaction in the browser. Use relevant wide and narrow viewports; shared UI requires both. Shell composition also requires the default and override paths. Completion requires observing the interaction in every applicable viewport and composition path.

## Rust test placement

Put test bodies in dedicated files. Use integration tests by default; private unit tests use the adjacent `<module>_tests.rs` convention. Shared integration helpers live in `tests/common/mod.rs`.
