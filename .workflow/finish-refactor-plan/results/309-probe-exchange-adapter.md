# 309 Probe Exchange Adapter

## Scope

Sixth A7 implementation slice: add a reusable App Server Protocol probe adapter
that builds `client/probe` JSON-RPC requests and parses typed probe responses.

## Contract

- Build `client/probe` requests in shared `app_server_client` code.
- Let concrete transports own supported transport selection, auth placement,
  reachability, and byte movement.
- Map successful responses to `Alive`.
- Map unauthorized responses to `AuthFailed`.
- Preserve explicit unreachable transport outcomes.
- Treat malformed JSON-RPC, wrong ids, and invalid payloads as hard probe
  errors.

## Non-Goals

- No HTTP, websocket, or stdio transport implementation.
- No launcher integration.
- No endpoint record registration.

## Next

Commit this slice, then implement the first concrete local endpoint exchange
that can move this `client/probe` request over a reusable local transport.

## Implementation

- Added `app_server_client::probe::exchange`.
- Added `ClientProbeExchange` as the narrow byte-movement boundary.
- Added `ClientProbeProtocolTransport<T>` implementing `EndpointTransportProbe`.
- Builds a typed JSON-RPC `client/probe` request with stable request id.
- Parses JSON-RPC success responses whose `result` is a
  `ResponseEnvelope<ClientProbeResult>`.
- Maps unauthorized protocol errors to `AuthFailed`.
- Preserves explicit exchange-level `Unreachable` and `AuthFailed` outcomes.
- Rejects malformed JSON-RPC, wrong ids, ambiguous result/error responses, and
  invalid payloads as hard probe errors.

## Review

- First independent review found two correctness issues:
  - success parsing used a flat result shape instead of the real protocol-edge
    `ResponseEnvelope` under JSON-RPC `result`;
  - responses containing both `result` and `error` could map to `AuthFailed`.
- Fixes:
  - parse JSON-RPC `result` as `ResponseEnvelope<ClientProbeResult>`;
  - reject responses with both top-level `result` and `error`;
  - update tests to use the real wire shape and cover ambiguous responses.
- Follow-up independent review: clean.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_client::probe::exchange --lib`
- `cargo test -p openaide-runtime app_server_client --lib`
- `jq empty .workflow/finish-refactor-plan/state.json && git diff --check`
- Source-size scan for `app_server_client` production files.
