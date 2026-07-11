# P243 Backend ACP Active Session Registry Split

## Contract

Split ACP active-session registry operations out of
`openaide-rs/app-server/src/agent/acp_active_session_manager.rs` while
preserving `AcpActiveSessionManager` as the stable active Agent session runtime
facade.

Move session-map lookup, insert, removal, duplicate-id handling,
cancellation/close/delete dispatch, event-sink attachment lookup, and shutdown
close-task extraction into a focused private registry module.

Keep ACP worker startup, registry config lookup, trace/auth state, startup
timeout handling, terminal error recording, and runtime thread spawning in
`AcpActiveSessionManager`.

Preserve every public method signature, duplicate active-session error text,
not-ready error text, close-on-duplicate behavior, idempotent cancel/close
behavior, delete requiring active session, resume capability behavior, shutdown
close-task behavior, and existing tests. Do not change Agent runtime behavior,
ACP worker protocol, storage, protocol shapes, or App Server lifecycle in this
slice.

## Implementation

Implemented. `AcpActiveSessionManager` remains the active Agent session runtime
facade and keeps ACP startup, config lookup, auth/trace state, worker spawning,
startup timeout handling, and terminal error recording. The new private
`AcpActiveSessionRegistry` owns active-session map lookup, insert, removal,
missing-session errors, cancel/close/delete dispatch, prompt/sink lookup, and
shutdown close-task extraction.

## Review

`$doomsday-review` ran correctness, requirements/tests, and code-quality
subagent passes. Correctness and code quality reported no findings. The
requirements/tests pass found missing edge-case coverage for inactive registry
operations. Added focused tests for missing-session resume, event-sink attach,
prompt, delete, and idempotent missing cancel/close.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime agent::acp::tests::active_session_runtime -- --nocapture`
- `npm run check`
- `npm test`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- production source-size scan

Follow-up requirements review reported no findings after the test-gap fix.

## Status

Completed and ready to commit.
