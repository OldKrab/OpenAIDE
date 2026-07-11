# Task Mutation Commit Review Loop

Date: 2026-06-26

## First Review Pass

Ran `$doomsday-review` style review with three subagents:

- Correctness found build/split timing and atomicity issues.
- Requirements/tests found missing proof for callback side-effect rollback, delete no-op behavior, and committed-task native delete ordering.
- Code quality found callback side-effect atomicity, shallow seam concerns, and `turns.rs` file size.

Fixes applied:

- Added `turn_events.rs` and declared it in `tasks/mod.rs`.
- Split `turns.rs` to 192 lines.
- Added message-file backup/restore around commit callbacks.
- Added invariant checks for Task identity and commit-managed version fields.
- Added mutation rollback tests.
- Added runtime contract tests for repeated archive/restore/delete no-ops.
- Added native-session delete ordering proof by making the fake agent observe the durable tombstone before delete.
- Made `notify_task_updated` private inside `TaskMutations`.

## Second Review Pass

Reran `$doomsday-review` after fixes:

- Independent correctness subagent returned no findings.
- Requirements and code-quality rerun subagents hit the temporary usage cap before completion.
- Requirements and code-quality were then rerun locally using the same doomsday references and current diff.

Local rerun result:

- Requirements/tests: no remaining findings for this slice. The new tests prove rollback, no-op revision stability, and native-delete commit ordering.
- Code quality: no blocking findings for this slice. The remaining `TaskTurnLifecycle` direct write/notify surface is documented migration scope for a later slice, not introduced as a new bypass by this change.

