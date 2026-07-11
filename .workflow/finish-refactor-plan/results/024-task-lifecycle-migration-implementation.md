# P03 Task Lifecycle Migration Implementation

Completed: 2026-06-26T22:37:42+03:00

## Implemented

- Migrated prompt follow-up durable Task and Chat writes to
  `TaskMutations::commit_existing_task`.
- Added `TaskMutations::create_task` and `create_task_with_validation` for new Task
  records plus initial normalized Chat history.
- Moved prompt-start and adopted-session creation durable commits to the Task mutation
  seam.
- Kept Agent side effects in `TaskTurnLifecycle`, `TurnRunner`, and `AgentGateway`.
- Split lifecycle creation code into `tasks/turn_lifecycle/create.rs` without widening
  `TaskTurnLifecycle` fields.
- Added a narrow creation validation context under `tasks/mutation/create_validation.rs`
  so adopted-session ownership validation does not expose raw storage to lifecycle
  callers.
- Made permission response routing commit-aware: the live waiter decision is stable
  while the durable permission mutation runs, the waiter is resolved only after commit
  succeeds, and commit failure leaves the waiter registered.
- Preserved resumed native sessions on follow-up attach failure instead of closing a
  shared session that another accepted turn may own.

## Tests Added Or Updated

- Create-task initial Chat persistence and real rollback after post-append write
  failure.
- Static migration guards for direct notifier, revision, task write, direct message
  persistence, and creation snapshot bypasses.
- Permission waiter routing tests for commit failure and concurrent response
  classification.
- Runtime contract for follow-up attach failure preserving a resumed native session.

