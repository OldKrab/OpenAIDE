# A1 Protocol Edge Runtime Path

## Contract

Add a real App Server Protocol stdio runtime path that parses JSON-RPC lines into
`protocol_edge::InboundProtocolMessage`, calls `RpcGateway`, and writes JSON-RPC
result/error envelopes produced from `openaide-app-server-protocol` response
records.

The App Server Protocol wire shape for this slice is JSON-RPC 2.0 with the
product method in the JSON-RPC `method`, method params in `params`, and optional
App Server Protocol `RequestMeta` in a top-level `meta` field. Responses keep
JSON-RPC `id` and put the typed App Server Protocol `ResponseEnvelope` or
`ErrorEnvelope` under `result` or `error` respectively.

The first runtime path supports only `client/initialize`, `state/subscribe`, and
`state/unsubscribe`, matching the current `RpcGateway` capability. Other product
methods must return the protocol-edge unsupported-method error and must not fall
through to legacy dotted dispatch.

Legacy `transport::dispatch::Dispatcher` may remain as a temporary compatibility
path for the current VS Code App Shell until the Frontend migration slices consume
the new path. A1 must not add new product behavior to the legacy dotted protocol.

## Status

Completed.

## Implementation

- Added `protocol_edge::stdio::ProtocolEdgeStdioDispatcher` as the JSON-RPC
  stdio adapter for the App Server Protocol edge.
- Added explicit `OPENAIDE_RUNTIME_PROTOCOL=app-server-protocol` binary mode in
  `main.rs` so the new protocol path is live without extending the legacy
  dotted dispatcher.
- Added protocol-edge stdio tests for initialize, initialize gating,
  unsupported product methods, invalid JSON, invalid JSON-RPC version, strict
  request id validation, and notifications.
- Added a runtime contract test that starts the `openaide-runtime` binary in
  App Server Protocol mode and initializes over stdio.

## Review

Round 1 found one accepted Important issue: JSON-RPC ids were represented as
`Option<Value>`, so explicit `null` ids were treated like notifications and
object or array ids could be stringified into gateway ids. Fixed by adding a
presence-aware constrained id parser: absent id is a notification, string/number
ids are accepted, and `null`, array, object, or bool ids return stable invalid
request errors.

Round 2 found invalid object/array ids were still echoed into response ids,
which would produce invalid JSON-RPC responses. Fixed invalid id responses to
always use `id: null`.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime protocol_edge -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge_runtime_mode_initializes_over_stdio -- --nocapture`
- `npm run check`
- `npm test`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`
- Production source-size scan, excluding tests/generated/dist/examples/target/node_modules.

## Next

Start A2: real renderable snapshots and committed `state_sync` event
publication.
