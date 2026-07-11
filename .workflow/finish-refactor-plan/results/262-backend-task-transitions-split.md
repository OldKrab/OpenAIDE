# Backend Task Transitions Split

## Contract

Split focused Task transition workflow groups out of
`openaide-rs/app-server/src/tasks/transitions.rs` while preserving
`TaskTransitions` as the stable lifecycle transition facade.

Ownership:

- `transitions.rs`: facade, `TaskTransitions` storage, and constructor.
- `transitions/active_turn.rs`: active-turn lookup, cancel-running transition,
  finish-turn transition, and active-turn matching.
- `transitions/recovery.rs`: recoverable Task record listing, volatile runtime
  recovery, and durable Agent session binding cleanup.
- `transitions/failure.rs`: created-task start failure and adopted-session
  attach failure finalization.
- `transitions/helpers.rs`: shared chat commit options and interruption message
  append helper.

Do not change public method names/signatures, lock timing, commit options, Task
status/unread/timestamp mutations, interruption reason/message/recoverability,
pending permission cancellation, running activity completion/error status,
archived-task recovery coverage, `TaskNotFound` handling for adopted attach
finalization, active turn matching behavior, Task mutation semantics,
TurnRunner behavior, storage records, runtime recovery policy, protocol shapes,
or existing tests in this slice.

Focused tests:

- Task mutation boundary tests cover mutation routing rules.
- Turn lifecycle and turns tests cover cancel/finish paths.
- Runtime shutdown/recovery contract tests cover volatile recovery and durable
  session binding cleanup.
- Task creation failure contract tests cover create/adopt finalization paths.

## Implementation

Implemented the split by keeping `transitions.rs` as the facade and moving
active-turn transitions, recovery/session-binding cleanup, failure finalization,
and shared helpers into focused private modules.

Production source sizes after split:

- `transitions.rs`: 17 lines.
- `transitions/active_turn.rs`: 101 lines.
- `transitions/recovery.rs`: 76 lines.
- `transitions/failure.rs`: 77 lines.
- `transitions/helpers.rs`: 28 lines.

## Review

`$doomsday-review`:

- Correctness/spec/tests: no findings.
- Code quality: local pass found no findings.

## Verification

Focused checks already run:

- `cargo fmt --all --check`: pass.
- `cargo check -p openaide-runtime`: pass.
- `cargo test -p openaide-runtime tasks::mutation::tests::task_turn_lifecycle_has_no_direct_commit_bypasses -- --nocapture`: pass.
- `cargo test -p openaide-runtime tasks::turns -- --nocapture`: pass.
- `cargo test -p openaide-runtime runtime_startup_recovers_stale_active_turn_and_session_binding -- --nocapture`: pass.
- `cargo test -p openaide-runtime shutdown_stops_active_turn_without_failed_task_state -- --nocapture`: pass.
- `cargo test -p openaide-runtime task_create -- --nocapture`: pass.
- `cargo test -p openaide-runtime shutdown -- --nocapture`: pass.
- `cargo test -p openaide-runtime recover -- --nocapture`: pass.

Final checks:

- `npm run check`: pass.
- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is
  `transitions/active_turn.rs` at 101 lines.

## Commit

This commit: `refactor: split backend task transitions`.

## Next

After this slice is committed, select the next compact refactor slice from the
current plan and architecture/file-size pressure.
