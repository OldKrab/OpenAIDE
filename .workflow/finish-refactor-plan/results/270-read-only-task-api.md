# A3a Read-Only Task API

## Contract

Implement the first read-only part of A3 without pretending the mutating Task
workflow is done.

- `task/list` and `task/open` are real App Server Protocol methods handled by
  `protocol_edge::RpcGateway`.
- The gateway remains thin: parse typed params, call injected Task snapshot
  sources, and serialize typed protocol results or errors.
- Storage-backed Task projections may read legacy durable records but must emit
  the new protocol shapes.
- Chat is part-based in protocol output.
- Attachment projection must not expose raw local paths.
- Live-session-only sections may render explicit unavailable or blocked states
  until the mutating runtime workflow is migrated.
- `task/create`, `task/send`, `task/setConfigOption`, `task/cancel`, and
  `task/discard` remain unsupported in this slice and must not fall through to
  legacy dotted behavior.

## Status

Completed.

## Implementation

- Added `snapshots::TaskSnapshotSource` and storage-backed `TaskSnapshotStore`.
- Added durable Task snapshot projection from legacy Task/Chat storage into the
  new App Server Protocol `TaskSnapshot`.
- Split chat projection into `snapshots::task_snapshot::chat_projection`.
- Injected the Task snapshot source into `RpcGateway`.
- Added `task/list` and `task/open` protocol-edge handlers.
- Added stdio protocol-edge tests for storage-backed `task/list` and
  `task/open`.
- Added strict storage listing for the new Task API projection so corrupt task
  records return recoverable protocol errors instead of disappearing.

## Review

Round 1 found two accepted issues:

- Corrupt task records could be silently omitted from `task/list` because the
  legacy list path tolerates malformed task metadata. Fixed by adding a strict
  storage list path for App Server Protocol projections and a regression test.
- Unsupported mutating methods were only tested through `task/create`. Fixed by
  covering `task/create`, `task/send`, `task/setConfigOption`, `task/cancel`,
  and `task/discard`.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo fmt --all`
- `cargo test -p openaide-runtime snapshots::task_snapshot -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::unsupported_mutating_task_methods_do_not_fall_through_to_legacy_dispatch -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::task_list_returns_storage_backed_tasks_after_initialize -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::task_open_returns_storage_backed_task_snapshot_after_initialize -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test --workspace -- --test-threads=1`
- `npm run check`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Next

Continue A3 with the mutating `task/create` and `task/send` workflow contract.
