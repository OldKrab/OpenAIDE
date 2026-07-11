# Next Slice Selection: Task Mutation Commit Boundary Split

## Selected Slice

Split task commit transaction orchestration and persistence helpers out of
`tasks/mutation.rs` into a focused internal commit module.

Tentative module shape:

- `tasks/mutation.rs`: public mutation facade, mutation context, public result
  and option types used by Task commands, turn lifecycle, transitions, and turn
  event handling.
- `tasks/mutation/commit.rs`: existing-task commit transaction, new-task
  creation transaction, message backup and restore coordination, revision
  assignment, response snapshot construction, and notification publication.
- `tasks/mutation/invariants.rs`: task identity and commit-managed version-field
  invariant checks, if that separation remains useful after grilling.
- `tasks/mutation/tests.rs`: focused tests remain outside production source
  files.

## Why This Slice

`tasks/mutation.rs` is the central task write boundary and currently mixes:

- facade construction and dependencies;
- mutation context methods exposed to callers;
- existing-task transaction orchestration;
- new-task creation transaction orchestration;
- message-file rollback coordination;
- revision assignment and commit publication;
- response snapshot construction;
- task invariant validation.

This matters more than another small line-count split because the mutation seam
is where storage, runtime revision state, chat history, and notifier publication
must remain coherent. Later Task, recovery, and attachment work will be easier
to review if the facade API is separated from the commit transaction machinery.

## Intended Boundary

`tasks/mutation.rs` should keep the caller-facing API:

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
- public mutation entry points:
  - `commit_existing_task`;
  - `create_task`;
  - `create_task_with_validation`;
  - test-only `append_message`.

The new commit module should own implementation details behind those entry
points:

- lock-scoped existing-task commit flow;
- lock-scoped new-task creation flow;
- storage message backup and restore calls;
- task invariant validation;
- revision candidate allocation and commit;
- task persistence;
- response snapshot construction;
- task-updated notification after successful durable commit.

Current callers should not import the new commit module directly. They should
continue to use `TaskMutations` and the existing mutation result types.

## Constraints

- No behavior changes.
- Keep `store_update_lock` semantics unchanged.
- Keep message side-effect rollback behavior unchanged for mutation errors,
  invariant failures, unchanged/rejected mutations, and create-task persistence
  failures.
- Keep revision allocation and commit behavior unchanged.
- Keep notification publication only after successful durable commit.
- Keep response snapshot behavior unchanged.
- Keep task-create validation timing unchanged.
- Keep existing test coverage and add focused tests only if the split exposes an
  unprotected edge.
- Keep production Rust source files under the 400-line limit.

## Main Risks To Grill

- Whether `TaskMutations` should expose a narrow internal dependency bundle to
  the commit module or pass dependencies per call.
- Whether `TaskMutationContext` should stay in the facade or move into the
  commit module with a re-export.
- Whether invariant validation belongs in the commit module or a separate
  `invariants` module.
- How to keep tests close to the public mutation facade while still proving the
  extracted commit transaction behavior.

## Next Step

Grill and record the API contract for the Task Mutation commit boundary split.
