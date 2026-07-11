# Task Turn Event Sink Split: Review Loop

## First Doomsday Review

Ran `$doomsday-review` with three subagents:

- Correctness: no findings.
- Requirements/tests: found that the implementation artifact claimed a focused
  permission rollback test that matched zero tests, and that append-failure
  waiter cleanup lacked direct coverage.
- Code quality: found that `PermissionWaiters` was still a raw
  `Arc<Mutex<HashMap<...>>>`, so permission registry lifecycle rules leaked out
  of `permissions.rs`.

## Fixes

- Replaced the raw `PermissionWaiters` type alias with an owned registry type in
  `tasks/turn_events/permissions.rs`.
- Moved permission response routing and commit-safe waiter resolution into the
  registry type.
- Updated `TurnRunner` and tests to use registry methods instead of locking the
  map directly.
- Added `tasks/turn_events/tests.rs` with direct coverage for
  `TaskEventSink::request_permission` removing the waiter when the permission
  message append/commit fails.
- Removed the false focused-test claim from the implementation artifact.
- After the second code-quality pass, narrowed `register`, `remove`,
  `PermissionWaiter`, and `PermissionWaiter::resolve` visibility so only
  `route_response` is crate-visible for `TurnRunner`. Tests use a `#[cfg(test)]`
  setup helper.

## Verification After Fixes

Passed:

- `cargo test -p openaide-runtime permission_request_append_failure_removes_waiter -- --nocapture`
- `cargo test -p openaide-runtime permission_response_route -- --nocapture`
- `cargo test -p openaide-runtime`

## Next Step

Run final integration verification and commit the completed slice.
