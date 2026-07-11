# Task Mutation Commit Split API Contract

## Decision

Split commit transaction implementation details out of `tasks/mutation.rs` into
`tasks/mutation/commit.rs`.

This is a structural refactor only. The caller-facing `TaskMutations` API and
all task write semantics must remain unchanged.

## Module Boundary

`tasks/mutation.rs` remains the public facade for task mutation callers and
owns:

- `TaskMutations`;
- `TaskMutationContext`;
- `TaskCommitOptions`;
- `TaskCommitResult`;
- `TaskCommitOutcome`;
- `TaskCommitFacts`;
- `TaskCommitRejection`;
- `TaskMutationResult`;
- `TaskMutations::new`;
- `TaskMutations::store`;
- `TaskMutations::lock`;
- `TaskMutations::current_revision` for tests;
- public entry-point methods:
  - `commit_existing_task`;
  - `create_task`;
  - `create_task_with_validation`;
  - test-only `append_message`.

`tasks/mutation/commit.rs` owns implementation details behind those entry
points:

- lock-scoped existing-task commit flow;
- lock-scoped new-task creation flow;
- task existence checks for create;
- task-create validation timing;
- message-file backup and restore coordination;
- mutation closure execution;
- unchanged/rejected mutation handling;
- task invariant validation;
- changed-task persistence;
- new-task initial message persistence;
- revision candidate allocation and commit;
- task-updated notification publication;
- optional response snapshot construction.

No separate `tasks/mutation/invariants.rs` module is introduced in this slice.
The invariant helpers are private implementation details of `commit.rs` unless
future work gives them a broader contract.

## API Shape

`tasks/mutation.rs` delegates to child-module functions:

```rust
commit::commit_existing_task(self, task_id, options, mutation)
commit::create_task_with_validation(self, task, initial_messages, options, validate)
```

`TaskMutations::create_task` remains a facade convenience that calls
`create_task_with_validation` with a no-op validator.

The commit module receives `&TaskMutations` rather than a new dependency bundle.
Because it is a child module, it can access the facade's private fields and
does not need another pass-through struct for `Store`, lock, runtime state, and
notifier.

The commit module may define private helpers such as:

- `persist_changed_task`;
- `persist_new_task`;
- `notify_task_updated`;
- `validate_task_invariants`;
- `VersionFields`.

Those helpers must remain private to `commit.rs`.

## Behavior That Must Stay Unchanged

- `commit_existing_task` acquires the same store update lock before reading and
  mutating a task.
- Existing-task commits read the task before taking the message backup, as they
  do today.
- Mutation closure errors restore message files and return the original error.
- `TaskMutationResult::Changed` validates task identity and commit-managed
  version fields before persistence.
- Invariant failures restore message files and do not advance the global
  revision.
- `TaskMutationResult::Unchanged` and `TaskMutationResult::Rejected` restore
  message files and return `TaskCommitRejection::NoChange`.
- Persistence failures after message side effects restore message files and do
  not commit the runtime revision.
- Successful changed-task commits refresh message history only when
  `TaskCommitOptions::refresh_message_history` is true.
- Successful changed-task commits increment `task_version`, assign one new
  revision, write the task, commit the runtime revision, notify once, and return
  `TaskCommitFacts`.
- `create_task_with_validation` keeps the same lock scope.
- Create rejects an already-existing task id before validation.
- Create runs the validation callback before taking a message backup.
- Create takes a message backup before writing initial messages.
- Create persistence failures restore message files.
- Successful create appends initial messages, refreshes message history version,
  assigns one revision, writes the task through the supplied writer, commits the
  runtime revision, notifies once, and returns `TaskCommitFacts`.
- Optional response snapshots are built after commit outcome selection using the
  same tail limit behavior.
- `TaskMutations::store`, `TaskMutations::lock`, and test-only helpers retain
  their current behavior.

## Test Expectations

Keep the existing focused mutation tests in `tasks/mutation/tests.rs`. They
already cover the critical behavior this split must preserve:

- metadata commit revision and notification;
- unchanged and rejected commits;
- message-history refresh;
- task identity and version-field invariant failures;
- rollback of message side effects on rejected commits and invariant failures;
- create-task rollback and validation behavior.

Run at least:

- `cargo test -p openaide-runtime tasks::mutation -- --nocapture`;
- `cargo test -p openaide-runtime`;
- `cargo fmt --all --check`;
- `npm run check`;
- `git diff --check`.

## Rejected Directions

- Do not introduce a new commit dependency bundle for this slice.
- Do not expose commit helpers to callers outside `tasks::mutation`.
- Do not move `TaskMutationContext` out of the facade API.
- Do not split invariant helpers into their own module yet.
- Do not change storage backup/restore helpers in this slice.

## Next Step

Implement the commit split, then run doomsday-review against lock/rollback
semantics, revision/notification ordering, and module isolation.
