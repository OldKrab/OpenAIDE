# Task Turn Event Sink Split: Implementation

## Implemented

Split `tasks/turn_events.rs` into a facade plus focused child modules:

- `tasks/turn_events/streaming.rs` owns streamed text and thought run
  accumulation and message construction.
- `tasks/turn_events/permissions.rs` owns `PermissionWaiters`,
  `PermissionWaiter`, response resolution, and cancellation-aware waiting.
- `tasks/turn_events/config.rs` owns config-option update commits.

The facade still owns:

- `TaskEventSink`;
- `TaskSessionEventSink`;
- Agent event routing;
- permission request orchestration;
- message append/upsert commit calls;
- existing caller-facing imports.

## Preserved Behavior

- Text/thought chunks are still coalesced into one message per contiguous run.
- Switching between text, thought, config updates, permission requests, and other
  events still clears the same streaming runs.
- Tool call updates still default `scope_id` to the active `turn_id` and upsert
  by identity.
- Config updates still no-op when the active turn differs or cancellation has
  fired.
- Permission waiters are still cleaned up after append failure, cancellation, or
  resolution.

## Verification

Passed:

- `cargo test -p openaide-runtime tasks::mutation::tests::task_turn_lifecycle_has_no_direct_commit_bypasses -- --nocapture`
- `cargo test -p openaide-runtime permission_request_append_failure_removes_waiter -- --nocapture`
- `cargo test -p openaide-runtime permission_response_route -- --nocapture`
- `cargo test -p openaide-runtime`

Formatting was applied with `cargo fmt --all`.

## Next Step

Run `$doomsday-review` on this slice and fix material findings before final
integration verification.
