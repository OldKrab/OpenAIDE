# P06 Next Slice Selection

Completed: 2026-06-26T21:56:04+03:00

## Selected Slice

Finish the remaining `TaskTurnLifecycle` migration into the Task mutation commit seam.

## Why This Slice Is Next

`TaskTurnLifecycle` still owns direct durable Task workflow writes:

- follow-up prompt Chat writes, Task revision assignment, `Store::write_task`, and legacy notification;
- permission response message mutation, Task revision assignment, `Store::write_task`, and legacy notification;
- prompt-start Task creation with initial Chat messages and legacy notification;
- adopted-session Task creation with replayed Chat history and legacy notification.

Protocol-edge `state_sync` publication should not be introduced while these lifecycle
paths still bypass `TaskMutations`, because event publication would sit on inconsistent
commit facts.

## Slice Goal

Move `TaskTurnLifecycle` durable Task mutations through `TaskMutations` while keeping
Agent side effects in `TaskTurnLifecycle`.

## Scope

- Add a creation commit interface to `TaskMutations` for new Task records plus initial
  normalized Chat messages.
- Migrate `TaskTurnLifecycle::prompt` to `commit_existing_task`.
- Migrate `TaskTurnLifecycle::respond_permission` to `commit_existing_task`.
- Migrate `create_prompt_start` and `create_adopted_session` to the new creation commit
  interface.
- Remove `TaskTurnLifecycle` direct calls to `next_revision`, `Store::write_task`,
  `append_normalized_to_store`, and `RuntimeNotifier::task_updated`.
- Keep Agent session start/load/resume/close, event sink attach, turn spawning, and
  external cleanup policy in `TaskTurnLifecycle`.

## Out Of Scope

- No protocol-edge `state_sync` event publication yet.
- No broad Agent runtime or ACP behavior change.
- No shell-facing behavior change.
- No durable transaction engine beyond the current file-store commit seam.

