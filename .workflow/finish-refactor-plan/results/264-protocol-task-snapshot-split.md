# P242 Protocol Task Snapshot Split

## Contract

Split App Server Protocol Task snapshot records out of
`openaide-rs/app-server-protocol/src/snapshot/task.rs` while preserving
`openaide_app_server_protocol::snapshot::*` as the stable public namespace.

Keep `snapshot/task.rs` as the Task render-model facade and re-export owner.
Move preparation/setup records, live Agent config and slash-command records,
and send-capability records into focused private modules under
`snapshot/task/`. Preserve every Rust type name, serde shape, TypeScript
declaration name/order, public re-export, and generated protocol output.

Do not change App Server snapshot semantics, method or event records, runtime
behavior, storage behavior, or Frontend state ingestion in this slice.

## Implementation

Implemented. `snapshot/task.rs` remains the public Task snapshot facade and
re-export owner. Preparation/setup records, live Agent config and slash-command
records, and send-capability records now live in focused private modules under
`snapshot/task/`.

## Review

`$doomsday-review` was run with three scoped subagent passes: correctness,
requirements/tests, and code quality. All reported no findings.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-app-server-protocol`
- `cargo test -p openaide-app-server-protocol snapshot::tests -- --nocapture`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run check`
- `npm test`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- production source-size scan

## Status

Completed and ready to commit.
