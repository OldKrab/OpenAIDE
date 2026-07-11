# A3h Task Preparation Readiness

## Contract

Make `task/create` responsive without lying about Agent readiness.

- Return a durable Task immediately with renderable preparation state.
- Start Native Session preparation after the Task is durable.
- Publish normal Task updates when preparation becomes ready or failed.
- Block `task/send` and config mutations while preparation is not ready.
- Recover abandoned preparation after restart as a truthful failed state, not an
  infinite loading state.

## Status

Completed.

## Implementation

- Added durable `TaskPreparationRecord` to Task storage.
- Added `tasks::product_api::prepare` for background preparation, restart
  recovery, Send gating, and stale preparation cleanup.
- `task/create` now returns a preparing snapshot immediately and starts
  background Native Session preparation.
- Successful preparation stores the Native Session id, options, model, and
  ready state; failed preparation stores a recoverable failed state.
- `task/send` and `task/setConfigOption` reject Tasks whose preparation is not
  ready.
- Late successful preparation for a discarded Task closes the newly started
  Native Session and leaves the tombstoned Task untouched.
- Split Task readiness projection into `snapshots::task_snapshot::readiness`.

## Review

- Initial review found abandoned preparing Tasks after restart, late
  preparation mutating discarded Tasks, and config changes racing with
  preparation.
- Fixes added startup recovery, tombstone-aware preparation commit predicates,
  session close on stale preparation completion, and config mutation gating.
- Focused re-review reported no findings.

## Verification

- `cargo fmt --all`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test -p openaide-runtime snapshots::task_snapshot -- --nocapture`
- `cargo fmt --all --check`
- `cargo test --workspace -- --test-threads=1`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Commit this sub-slice, then continue A3 with slash-command readiness and richer
Agent option metadata.
