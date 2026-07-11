# Task Mutation Commit Split Implementation

## Summary

Implemented the accepted Task Mutation commit boundary split as a structural
refactor with no intended behavior changes.

## Code Changes

- Added `tasks/mutation/commit.rs` for:
  - existing-task commit transaction flow;
  - create-task transaction flow;
  - message backup and restore coordination;
  - mutation result handling;
  - task invariant validation;
  - changed-task and new-task persistence;
  - revision assignment and commit;
  - task-updated notification publication;
  - optional response snapshot construction.
- Kept `tasks/mutation.rs` as the caller-facing facade for:
  - `TaskMutations`;
  - `TaskMutationContext`;
  - commit result and option types;
  - public mutation entry points.
- Updated the mutation boundary test so the single allowed `task_updated`
  publisher is now `tasks/mutation/commit.rs`.

## Behavior Preservation

The implementation preserves:

- existing `TaskMutations` caller API;
- store update lock use around existing-task and create-task commit flows;
- duplicate task-id check before create validation;
- create validation before message backup;
- message rollback on mutation errors, invariant failures, rejected/unchanged
  mutations, and create persistence failures;
- revision candidate allocation before task write and revision commit after
  successful task write;
- notification publication only after successful durable commit;
- response snapshot construction behavior and tail limits.

## File Size Check

Production Rust files after the split:

- `tasks/mutation.rs`: 211 lines;
- `tasks/mutation/commit.rs`: 214 lines.

Both are below the 400-line production source file limit.

## Next Step

Record the doomsday-review result and integration verification, then commit the
slice.
