# P02 Task Turn Lifecycle Migration API Contract

Completed: 2026-06-26T21:56:04+03:00

## Selected Slice

Finish migrating `TaskTurnLifecycle` durable Task mutations into `TaskMutations`.

## Accepted Interfaces

- `TaskTurnLifecycle` remains the owner of Agent-facing workflow policy:
  - validate create/prompt parameters;
  - start, load, resume, close, and delete Agent sessions through `AgentGateway`;
  - attach session event sinks;
  - spawn Agent turns through `TurnRunner`;
  - run failure cleanup when Agent/session setup fails.
- `TaskMutations` owns all durable Task and Chat commit mechanics for this slice:
  - in-process mutation lock;
  - Task revision assignment;
  - message history version refresh;
  - initial Chat message persistence;
  - Task record persistence;
  - legacy `RuntimeNotifier::task_updated` publication while the app still uses it;
  - response snapshot construction when requested.
- New Task creation uses a new creation commit method, conceptually:
  `create_task(record, initial_messages, options) -> TaskCommitResult`.
- Existing Task follow-up prompt and permission response use
  `commit_existing_task(...)`.
- Commit callbacks may read and mutate workflow-owned Task fields, but must not set
  Task identity, revision, task version, or message history version. `TaskMutations`
  keeps enforcing those invariants.
- Any Chat writes performed inside a commit callback must remain covered by the
  existing rollback behavior for rejected/failed commits.
- `TaskCommitOutcome` remains the only post-commit publication fact surface. Callers
  may run external post-commit side effects, such as spawning a turn, only after a
  committed outcome.

## Ordering Rules

- Prompt follow-up:
  1. Read enough Task state to decide whether to start or resume an Agent session.
  2. Start/resume and attach Agent session outside the mutation lock.
  3. Commit the user message, running turn activity, active turn fields, session id,
     and Task status through `TaskMutations`.
  4. Spawn the Agent turn only after the commit succeeds.
  5. If attach or commit fails after opening an Agent session, close or invalidate using
     the existing cleanup paths.
- Prompt-start creation:
  1. Validate Agent/options and start the Agent session.
  2. Commit the new Task record and initial Chat messages through `TaskMutations`.
  3. Attach events and spawn the turn only after durable commit succeeds.
  4. If later setup fails, close/finalize through existing failure paths.
- Adopted-session creation:
  1. Validate the external session is unowned.
  2. Load the Agent session and replayed history.
  3. Recheck ownership and commit the new Task plus replayed history through
     `TaskMutations`.
  4. Attach events only after durable commit succeeds.
  5. If commit or attach fails, close/finalize through existing failure paths.
- Permission response:
  1. Resolve the durable permission message and route the live waiter result inside one
     Task mutation commit.
  2. If denied after the live waiter is gone, append the interruption inside that same
     commit.
  3. Return a snapshot produced under the commit interface.

## Required Tests

- Follow-up prompt no longer has direct `RuntimeNotifier::task_updated`,
  `next_revision`, `Store::write_task`, or `append_normalized_to_store` calls in
  `TaskTurnLifecycle`.
- Permission response no longer has direct durable Task write or notification calls in
  `TaskTurnLifecycle`.
- Prompt-start and adopted-session creation use the creation commit interface and still
  preserve existing runtime contract behavior.
- A create commit write failure does not advance global revision, does not publish
  notification facts, and closes any already-opened Agent session through existing
  cleanup.
- A follow-up prompt commit failure closes/invalidates the Agent session and does not
  spawn a turn.
- A permission response rejected/failing commit does not leave Chat side effects outside
  the committed outcome.
- The temporary direct notifier allowlist shrinks to zero or to only explicitly
  documented paths that remain unmigrated after this slice.

## Next

Proceed to `P03-implementation-slice`: implement the accepted lifecycle migration slice,
then run `$doomsday-review` before committing.

