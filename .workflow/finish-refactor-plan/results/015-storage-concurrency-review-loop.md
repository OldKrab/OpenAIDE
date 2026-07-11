# P04 Storage Concurrency Review Loop

Completed: 2026-06-26T20:35:06+03:00

## Scope

Ran the doomsday-review loop for the storage concurrency implementation slice against
`HEAD~1...HEAD`, including correctness, requirements/tests, and code-quality subagent
passes plus a local lifecycle invariant pass.

## Findings Resolved

- Replaced stale sentinel writer locks with OS advisory file locks through `fs2`, so
  crash recovery is not blocked by an abandoned lock file.
- Changed storage clean marking from unconditional `Drop` behavior to explicit coherent
  shutdown behavior through `TaskService::shutdown`.
- Made `Store::open` expose structured `StoreOpenError` directly and removed the
  obsolete `open_checked` split.
- Restored runtime recovery assertions that prove stale durable `active_turn_id` and
  `agent_session_id` fields are cleared.
- Moved storage runtime marker schema, marker writes, and lock/open-state mechanics
  behind `storage_runtime`, leaving product `storage::Store` as the guarded product
  storage facade.
- Trimmed `RecoveryClassification` to successful open recovery states; open failures
  are represented by structured `StorageOpenError`.
- Blocked JSON-RPC batch requests from mutating storage after `runtime.shutdown`
  writes the clean marker.
- Updated the implementation record so it no longer describes the obsolete clean-drop
  and `open_checked` behavior.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime storage_runtime::tests -- --nocapture`
- `cargo test -p openaide-runtime storage::tests -- --nocapture`
- `cargo test -p openaide-runtime transport::dispatch::tests::batch_requests_after_shutdown_are_rejected_without_mutating_storage -- --nocapture`
- `cargo test -p openaide-runtime --test runtime_contract -- --nocapture`
- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`
- Source-size scan: touched production files remain below the 400-line hard cap.

## Review Result

Findings

No findings after the resolved doomsday-review rerun findings above.

Summary: 0 findings: 0 correctness, 0 requirements/tests, 0 code quality.

## Next

Proceed to `P05-integration-verification`: finalize workflow docs, inspect the final
diff, and commit the storage concurrency review fixes.
