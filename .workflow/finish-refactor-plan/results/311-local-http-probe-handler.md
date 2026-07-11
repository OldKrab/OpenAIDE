# 311 LocalHttp Probe Handler

## Scope

Eighth A7 implementation slice: add a server-side LocalHttp handler for
authorized `client/probe` JSON-RPC requests.

## Contract

- Validate `Authorization: Bearer <process-token>` before parsing protocol
  content.
- Route authorized `client/probe` requests through `RpcGateway`.
- Return JSON-RPC response bodies compatible with `LocalHttpProbeExchange`.
- Return HTTP 401/403 for missing or invalid local transport auth.
- Return HTTP 400 for malformed JSON-RPC.
- Do not add a socket listener, endpoint writer, launcher integration, or normal
  product traffic transport.

## Non-Goals

- No HTTP listener.
- No endpoint record registration.
- No shell integration.
- No product request routing beyond `client/probe`.

## Next

Commit this slice, then add the socket listener that uses this handler and
publishes a `LocalHttp` runtime endpoint record.

## Implementation

- Added `protocol_edge::local_http`.
- Added `LocalHttpProbeHandler` wrapping `RpcGateway` with process-token auth.
- Added pure request handler for focused tests.
- Validates Authorization before JSON-RPC parsing.
- Routes only `client/probe` to the gateway.
- Shapes success and error JSON-RPC bodies for `LocalHttpProbeExchange`.
- Returns empty HTTP 401/403 bodies for missing or invalid auth.

## Review

- Independent review: clean.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime protocol_edge::local_http --lib`
- `cargo test -p openaide-runtime protocol_edge --lib`
- `jq empty .workflow/finish-refactor-plan/state.json && git diff --check`
- Protocol-edge production source-size scan.
