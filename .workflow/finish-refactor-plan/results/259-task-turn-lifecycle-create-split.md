# Task Turn Lifecycle Create Split

## Contract

Split focused create workflow modules out of
`openaide-rs/app-server/src/tasks/turn_lifecycle/create.rs` while preserving
`TaskTurnLifecycle::create_prompt_start` and
`TaskTurnLifecycle::create_adopted_session` as lifecycle-internal workflow entry
points.

Ownership:

- `create.rs`: create-workflow facade, config-option request helper, and shared
  snapshot commit options.
- `create/prompt_start.rs`: first-prompt Task creation, Agent session start,
  initial user/running messages, event attachment, failure finalization, and
  turn spawning.
- `create/adopted_session.rs`: external native session adoption, Agent session
  load, ownership validation, replay persistence, event attachment, and
  adoption failure finalization.
- `create/helpers.rs`: create-title derivation and required optional text
  validation helpers.

Do not change validation order, Agent session start/load params, config option
behavior, TaskRecord fields, message order, session guard close/commit behavior,
attach/finalize error handling, spawned turn behavior, Task mutation semantics,
Agent runtime behavior, storage records, protocol records, or public service
APIs in this slice.

Focused tests:

- `tasks::mutation::tests::task_turn_lifecycle_has_no_direct_commit_bypasses`
  covers the mutation-boundary rule across recursive `turn_lifecycle` modules.
- Runtime contract Task creation/adoption tests cover behavior preservation.

## Implementation

Implemented the split by keeping `create.rs` as the facade and moving the two
create workflows plus shared helpers into private modules under
`src/tasks/turn_lifecycle/create/`.

Production source sizes after split:

- `create.rs`: 30 lines.
- `create/prompt_start.rs`: 122 lines.
- `create/adopted_session.rs`: 128 lines.
- `create/helpers.rs`: 45 lines.

## Review

`$doomsday-review`:

- Correctness/spec/tests: no findings.
- Code quality: local pass found no findings.

## Verification

Focused checks already run:

- `cargo fmt --all --check`: pass.
- `cargo check -p openaide-runtime`: pass after tightening moved workflow method visibility.
- `cargo test -p openaide-runtime tasks::mutation::tests::task_turn_lifecycle_has_no_direct_commit_bypasses -- --nocapture`: pass.
- `cargo test -p openaide-runtime task_create -- --nocapture`: pass.
- `cargo test -p openaide-runtime first_prompt_creates_durable_task_and_chat_history -- --nocapture`: pass.

Final checks:

- `npm run check`: pass.
- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is
  `create/adopted_session.rs` at 128 lines.

## Commit

This commit: `refactor: split task turn lifecycle create`.

## Next

After this slice is committed, select the next compact refactor slice from the
current plan and architecture/file-size pressure.
