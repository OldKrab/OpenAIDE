# A2 Task Navigation Snapshots

## Contract

This slice implements the first real renderable snapshot path without taking on
the entire product-state backlog.

- `SnapshotBuilder` stays projection-only: it may read through injected snapshot
  sources, but it must not mutate storage, start Agent runtime work, or make App
  Shell decisions.
- Task Navigation snapshots are backed by durable task storage and are returned
  from both `client/initialize` and `state/subscribe`.
- Snapshot read failures return recoverable protocol errors rather than empty
  successful snapshots.
- Transitional Project identity is derived as an opaque stable id from the
  stored workspace root because App Server-owned Project records are still an
  A8 slice.
- Task Navigation subscriptions honor their optional Project filter for
  snapshots. Full-list Task Navigation update events are delivered only to
  unfiltered Task Navigation subscriptions until Project-owned event routing is
  designed.
- State-sync publication stays generic: callers build a typed Task Navigation
  payload after durable acceptance and pass it to `publish_committed`.
- Legacy `RuntimeNotifier` remains only as a temporary compatibility path until
  the target `task/*` API can call state sync directly.

## Status

Completed.

## Implementation

- Added `snapshots::TaskNavigationSnapshotSource` and storage-backed
  `TaskNavigationStore`.
- Injected Task Navigation snapshot source into `SnapshotBuilder`.
- Made App Server Protocol stdio startup open `Store` and pass
  `TaskNavigationStore` into `RpcGateway`.
- Propagated fallible snapshot reads through `client/initialize` and
  `state/subscribe`.
- Kept `StateStream` generic and tested committed Task Navigation publication by
  building the typed payload outside state sync.
- Added unit tests for snapshot projection, Project filtering, storage read
  errors, committed publication, and storage-backed initialize output.

## Review

Round 1 found two accepted correctness issues and one accepted architecture
cleanup:

- Storage read failures were converted to empty navigation snapshots. Fixed by
  making Task Navigation snapshot reads fallible and returning recoverable
  protocol errors through initialize and subscribe.
- Project-filtered Task Navigation subscriptions received global task data.
  Fixed by filtering subscription snapshots and by not delivering full-list
  Task Navigation updates to project-filtered subscriptions.
- `StateStream` briefly contained a task-specific publication helper. Fixed by
  removing it and using generic `publish_committed` with an externally built
  payload.

Two lower-priority notes remain documented tradeoffs for later slices:

- `ProtocolEdgeStdioDispatcher` currently performs storage/snapshot composition
  until the reusable app-server composition layer exists.
- Project ids are transitional opaque hashes of legacy workspace roots until A8
  introduces App Server-owned Project records.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo fmt --all`
- `cargo test -p openaide-runtime snapshots::task_navigation -- --nocapture`
- `cargo test -p openaide-runtime state_sync -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge_runtime_mode_initializes_over_stdio -- --nocapture`
- `npm run check`
- `npm test` exposed intermittent unrelated ACP close timeouts in parallel
  backend tests; each failed test passed when rerun directly.
- `cargo test --workspace -- --test-threads=1`
- `npm run test --workspace @openaide/app-server-client`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- Production source-size scan, excluding tests/generated/dist/examples/target/node_modules.

## Next

Start A3: grill and implement the target `task/*` product API and split Task
create from send.
