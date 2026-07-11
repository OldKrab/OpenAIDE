# 314 Launch Handoff Loop

## Scope

Eleventh A7 implementation slice: add a shared attach-or-launch handoff loop
that lets shells wait for another launcher, retry endpoint discovery, or become
the elected launcher without duplicating endpoint and lock logic.

## Contract

- Keep handoff logic in the App Server client layer.
- Reuse concrete LocalHttp probing through `AttachOrLaunchRunner`.
- Return attach targets immediately when a compatible endpoint exists.
- Return a launch lock when this process is elected to launch.
- Wait and retry when another process owns the launch lock.
- Bound wait attempts by policy and return a typed still-in-progress error.
- Provide an injectable waiter for tests and a sleep-based default waiter for
  real bootstrap code.

## Implementation

- Added `app_server_client::launch_handoff`.
- Added `LaunchWaiter`, `SleepLaunchWaiter`, `LaunchHandoffPolicy`,
  `AttachOrLaunchHandoff`, `LaunchHandoffResult`, and typed errors.
- Added tests for launch election, immediate attach, busy-lock wait/retry with a
  real LocalHttp published endpoint, and bounded wait failure.
- Updated the refactor plan with the completed slice and next A7 step.

## Review

- Local review focused on lifecycle correctness, lock ownership, bounded waits,
  and avoiding shell-specific duplication.
- Adjusted the implementation to include a real default waiter so shells do not
  invent wait behavior.
- Tests use a real published LocalHttp probe endpoint for attach paths instead
  of relying on record-shape-only fixtures.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_client::launch_handoff --lib`
- `cargo test -p openaide-runtime app_server_client --lib`
- `git diff --check`
- Source-size scan for touched files.
