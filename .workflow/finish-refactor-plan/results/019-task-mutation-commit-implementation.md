# Task Mutation Commit Implementation

Date: 2026-06-26

Implemented the first narrow Task mutation commit seam slice.

## Scope

- Added `TaskMutations::commit_existing_task` as the commit boundary for existing Task records.
- Added closed commit result types for committed and non-committed outcomes.
- Added revision candidate/commit split so failed task writes do not advance global runtime revision.
- Migrated `task.markRead`, archive/restore/tombstone, Task transition helpers, active turn event writes, and config-option updates through the commit seam.
- Split turn event sink logic out of `turns.rs` into `turn_events.rs`.
- Added message-file backup/restore around commit callbacks so callback-side Chat/activity writes roll back when the mutation rejects or the final Task commit fails.
- Added invariant checks that reject Task identity and commit-managed version-field mutations from commit callbacks.

## Deliberate Remaining Migration Surface

- `TaskTurnLifecycle` create, prompt start, and permission response paths still use the older direct write/notify path. This is the explicit migration surface recorded in `docs/refactor-plan.md` for this first slice.
- `RuntimeNotifier::task_updated` remains migration plumbing until later `state_sync` event publication is wired in.

