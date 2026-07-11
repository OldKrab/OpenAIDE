# P02 Task Mutation Commit API Contract

Completed: 2026-06-26T20:37:47+03:00

## Selected Slice

Backend Task mutation commit seam and workflow split.

## Accepted Interfaces

- `task_mutations` is the single Task-workflow module that owns durable Task mutation
  commits. Task creation, prompt, cancel, permission response, delete/archive/restore,
  recovery, and Agent turn handlers call this module instead of open-coding lock,
  revision, write, message-history refresh, and notification behavior.
- The main external interface is a small commit surface:
  - execute a named Task mutation under the in-process mutation lock;
  - expose read access needed by that mutation through a controlled mutation context;
  - assign Task revisions exactly once for accepted Task changes;
  - refresh message history version when the mutation appends, upserts, or finalizes
    Chat state;
  - persist through `Store`;
  - return a closed `TaskCommitOutcome`.
- `TaskCommitOutcome` is the only thing outside `task_mutations` may use for
  post-commit publication. It includes the affected `task_id`, committed revision,
  whether Task navigation changed, whether a full Task snapshot is needed, whether a
  Chat item or chunk event can be emitted directly, and any follow-up snapshot reads
  required for the method result.
- Failed persistence returns an error and no `TaskCommitOutcome`; callers must not
  publish events, notify, clear pending UI as committed, or run post-commit side
  effects from failed commits.
- Rejected mutations that make no durable change return a closed no-op or rejected
  outcome without advancing Task revision, global revision, message-history version, or
  publication facts. Callers must not infer rejection from `Ok(None)` or optional
  fields.
- `task_mutations` returns publication facts; it does not directly own protocol
  delivery, transport routing, subscription state, or App Shell clients.
- `state_sync` remains the ordered event publisher. Later integration will translate
  `TaskCommitOutcome` into `AppServerEventPayload` and call
  `StateStream::publish_committed` only after durable commit succeeds.
- `RuntimeNotifier` is legacy notification plumbing during migration. New Task
  workflow code must not add direct `RuntimeNotifier::task_updated` calls; it should
  return or consume `TaskCommitOutcome`.
- During the first implementation, direct `RuntimeNotifier::task_updated` is allowed
  only in unmigrated legacy paths. The explicit temporary allowlist is:
  - `TaskTurnLifecycle::prompt`;
  - `TaskTurnLifecycle::respond_permission`;
  - `TaskTurnLifecycle::create_prompt_start`;
  - `TaskTurnLifecycle::create_adopted_session`.
  The migrated `TaskService::mark_read` and archive/restore/tombstone paths must not
  call `RuntimeNotifier::task_updated` directly.
- Workflow modules own workflow policy and external effects such as Agent session
  start, resume, close, delete, cancellation, and permission response routing. They do
  not own revision assignment or direct post-commit event publication.
- `Store` remains the durable product storage facade. It owns file persistence, but it
  does not decide workflow semantics, event payloads, or Task lifecycle transitions.
- The commit seam must preserve responsive UI requirements: accepted mutations return
  enough renderable state or snapshot-read instructions immediately, and long-running
  Agent work continues to stream through later events.

## First Implementation Scope

- Deepen the existing `tasks::mutation::TaskMutations` into the commit seam instead
  of creating a parallel abstraction.
- Move the simple Task-only write paths first: `mark_read`, archive/restore/tombstone
  metadata changes, and transition helpers that already call `TaskMutations`.
- Preserve existing method results and legacy notifications until `protocol_edge`
  event delivery is wired in a later slice.
- Add tests at the Task mutation interface proving revision assignment, message-history
  refresh decisions, and publication facts.

## Required Tests

- A Task metadata commit bumps `task_version` and global revision once, writes the Task,
  and returns a `TaskCommitOutcome` with Task update facts.
- A no-op mutation returns a no-op outcome and does not bump revision or notify.
- A rejected mutation returns an explicit rejected/no-op outcome and does not bump Task
  revision, global revision, message-history version, write storage, or produce
  publication facts.
- A failed store write returns an error and produces no `TaskCommitOutcome` or
  post-commit publication facts.
- A mutation that appends or changes Chat refreshes `message_history_version` before
  writing the Task.
- Task service `mark_read` and archive/restore/tombstone paths use the commit seam and
  keep their existing runtime contract behavior.
- New code does not introduce direct `RuntimeNotifier::task_updated` calls outside the
  explicit temporary allowlist, and migrated paths have no direct notifier calls.

## Next

Proceed to `P03-implementation-slice`: implement the narrow commit seam and migrate
only the selected Task write paths after reviewing this contract.
