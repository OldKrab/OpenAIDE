# P426 Shutdown Clean Release Wiring

## Status

Completed.

## Implementation

Added a coherent shutdown path for the local App Server lifecycle.

- Introduced `AppServerShutdownWorkflow` on the App Server gateway boundary.
- `TaskProductApi` implements shutdown by stopping live turns/Agent runtime and
  only then marking storage clean.
- `RpcGateway::shutdown` moves lifecycle to stopping, runs the shutdown
  workflow, reports clean completion only on success, and leaves unclean
  completion on failure.
- `SharedRpcGateway` exposes shutdown to process-level owners.
- LocalHttp last-client expiry now attempts clean shutdown; on clean release it
  removes the current runtime endpoint record and exits the process.
- Failed shutdown logs an error and does not remove the endpoint through the
  clean path or mark storage clean.

## Verification

Passed:

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime tasks::product_api::tests::shutdown_marks_storage_clean_after_task_runtime_shutdown`
- `cargo test -p openaide-runtime protocol_edge::tests::heartbeat_refreshes_client_liveness`
- `git diff --check`
- Rust production source-size guard

## Next Packet

P427 should audit the current plan and code for any remaining stale legacy
paths, missing integration checks, or evidence needed before the refactor plan
can be considered complete.
