# A3b Project Resolver Prerequisite

## Contract

Before `task/create` can be implemented correctly, the Backend must resolve the
typed `projectId` in `TaskCreateParams` to Task creation context without adding
raw workspace paths to the public Task API.

This slice adds the minimal resolver needed for A3b:

- Backend-owned and App Server internal.
- Resolves only Projects already known from durable Task history.
- Returns workspace root, safe project label, isolation default, and typed
  `ProjectId` for Task workflows.
- Uses strict task record reads so corrupt durable state returns a recoverable
  protocol error.
- Centralizes the transitional `ProjectId` derivation used by Task Navigation
  and Task snapshots.
- Leaves full Project records, project listing, new workspace selection, and
  durable Project settings to A8.

## Status

Completed.

## Implementation

- Added `projects::ProjectResolver`, `ProjectTaskContext`, and
  `StorageProjectResolver`.
- Moved transitional `project_id_for_workspace` into `projects`.
- Added strict all-task-record listing for resolver use without changing legacy
  permissive list behavior.
- Updated Task Navigation to use the centralized Project id helper.
- Added tests for resolving a known project, unknown project not found, and
  corrupt task record errors.
- Made resolver selection deterministic by choosing the newest matching
  non-tombstoned task using stable tie-breakers.

## Review

Round 1 found one accepted correctness issue: when several historical tasks for
the same workspace had different isolation values, resolver output depended on
filesystem iteration order. Fixed by sorting matching records by last activity,
updated time, and task id before choosing the Project task context.

## Verification

- `cargo fmt --all`
- `cargo test -p openaide-runtime projects -- --nocapture`
- `cargo test -p openaide-runtime snapshots::task_navigation -- --nocapture`
- `cargo test -p openaide-runtime snapshots::task_snapshot -- --nocapture`
- `cargo check -p openaide-runtime`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Wire the resolver into `task/create`.
