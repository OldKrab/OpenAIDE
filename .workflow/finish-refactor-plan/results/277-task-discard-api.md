# A3f Task Discard API

## Contract

Implement the first App Server Protocol `task/discard` path for empty pre-send
Task cleanup only.

- Accept typed `taskId`.
- Tombstone only empty pre-send Tasks.
- Reject running Tasks and Tasks with sent prompt or Chat history.
- Return refreshed Task Navigation.
- Publish accepted navigation events through state sync.
- Keep historical Task deletion and native-session deletion separate.

## Status

Completed.

## Implementation

- Added `TaskDiscardWorkflow`.
- Added `tasks::product_api::discard` through `TaskMutations`.
- Routed `task/discard` through protocol edge and stdio composition.
- Made tombstoned Tasks unavailable through App Server Protocol task open and
  product mutation paths.
- Added product API and stdio protocol-edge tests.

## Review

- Initial doomsday review found tombstoned Tasks could still be opened/sent by
  id, already tombstoned historical Tasks could be accepted by discard, running
  discard rejection lacked direct coverage, and discard eligibility was
  duplicated.
- Fixes added tombstone rejection at product boundaries, stricter discard
  eligibility, running/historical/tombstone regression tests, and a reusable
  eligibility helper that preserves storage errors.
- Follow-up correctness and requirements/tests reviews reported no findings.
- Final focused code-quality review reported no findings.

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

Commit this sub-slice, then continue A3 with Agent execution/readiness.
