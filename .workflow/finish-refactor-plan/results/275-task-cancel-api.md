# A3d Task Cancel API

## Contract

Implement the first App Server Protocol `task/cancel` path for persisted active
Task state.

- Accept typed `taskId` and optional `turnId`.
- Return the current Task snapshot when the Task is already idle.
- Reject mismatched `turnId` for an active Task.
- For the active turn, finish the running activity, cancel pending permissions,
  append a canceled interruption marker, clear `activeTurnId`, set the Task idle,
  and return a renderable Task snapshot.
- Publish accepted mutation events through state sync.
- Keep live Agent process cancellation for the later Agent execution/readiness
  sub-slice.

## Status

Completed.

## Implementation

- Added `TaskCancelWorkflow`.
- Added `tasks::product_api::cancel` routed through `TaskMutations`.
- Wired `task/cancel` into protocol edge Task handlers and stdio composition.
- Removed `task/cancel` from the unsupported-method guard.
- Added product API and stdio protocol-edge tests.

## Verification

- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test --workspace -- --test-threads=1`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Commit this sub-slice before continuing A3.
