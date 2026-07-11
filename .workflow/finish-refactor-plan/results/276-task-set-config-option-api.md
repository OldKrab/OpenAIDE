# A3e Task Set Config Option API

## Contract

Implement the first App Server Protocol `task/setConfigOption` path for idle
Tasks.

- Accept typed `taskId`, `configId`, `value`, and `clientMutationId`.
- Persist the selected config option on an idle Task through the shared mutation
  boundary.
- Return a renderable Task snapshot.
- Reject running Tasks until live Agent option application is wired.
- Publish accepted mutation events through state sync.

## Status

Completed.

## Implementation

- Added `TaskSetConfigOptionWorkflow`.
- Added `tasks::product_api::set_config_option` routed through
  `TaskMutations`.
- Wired `task/setConfigOption` into protocol edge Task handlers and stdio
  composition.
- Removed `task/setConfigOption` from the unsupported-method guard.
- Added product API and stdio protocol-edge tests.

## Verification

- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo fmt --all --check`
- `cargo test --workspace -- --test-threads=1`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Commit this sub-slice, then continue A3 with `task/discard`.
