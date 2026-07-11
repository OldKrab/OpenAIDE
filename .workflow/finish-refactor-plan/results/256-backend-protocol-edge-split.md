# Backend Protocol Edge Split

## Contract

Split message/outcome types and response/error construction helpers out of
`openaide-rs/app-server/src/protocol_edge.rs` while preserving `RpcGateway` as the stable
public protocol-edge coordinator.

Ownership:

- `protocol_edge.rs`: initialize gating, method dispatch, lifecycle admission, client
  registration, subscription handling, transport-close observation, and snapshot
  coordination.
- `protocol_edge/messages.rs`: inbound protocol message, gateway outcome, and gateway
  response types.
- `protocol_edge/responses.rs`: response envelope creation and stable protocol error
  construction helpers.

Do not change public type names re-exported from `protocol_edge`, method routes, error
codes/messages/recoverability/targets, response envelope meta behavior, snapshot cursor
behavior, App Server Protocol records, state lifecycle, client lifecycle, or existing
tests.

Focused tests:

- Existing `cargo test -p openaide-runtime protocol_edge -- --nocapture` covers the moved
  edge behavior.
- `cargo check -p openaide-runtime` covers Rust module/type boundaries.

## Implementation

Implemented the split by moving protocol-edge message/outcome types and response/error
construction into private submodules. `RpcGateway` remains the public coordinator and
still owns initialize gating, method dispatch, lifecycle admission, client registration,
subscription handling, transport-close observation, and snapshot coordination.

Production source sizes after split:

- `protocol_edge.rs`: 198 lines.
- `protocol_edge/messages.rs`: 38 lines.
- `protocol_edge/responses.rs`: 81 lines.

## Review

`$doomsday-review`:

- Correctness: no findings.
- Requirements/tests: accepted one Low missing-test finding for response meta and stable
  error detail preservation.
- Code quality: local pass found no findings.

Fix:

- Added focused protocol-edge assertions for response `client_request_id` meta,
  not-initialized message/recoverability/target, and invalid-params error meta/target.

## Verification

Focused checks already run:

- `cargo fmt --all`: pass.
- `cargo check -p openaide-runtime`: pass.
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`: pass.

Final checks:

- `cargo test -p openaide-runtime`: pass.
- `npm run check`: pass.
- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is `protocol_edge.rs` at 198
  lines.

## Commit

This commit: `refactor: split backend protocol edge helpers`.

## Next

After this slice is committed, select the next compact refactor slice from the current
plan and architecture/file-size pressure.
