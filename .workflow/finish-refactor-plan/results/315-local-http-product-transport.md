# 315 LocalHttp Product Transport

## Scope

Twelfth A7 implementation slice: turn the published LocalHttp endpoint from
probe-only into a basic reusable product request transport while preserving the
existing probe compatibility path.

## Contract

- Keep compatibility `client/probe` object responses for endpoint probing.
- Route requests with `X-OpenAIDE-Connection-Id` through the App Server Protocol
  product request handler.
- Use the supplied connection id as stable local HTTP transport identity inside
  `client_hub`.
- Return ordered JSON-RPC wire message arrays for product requests so responses,
  events, and Backend-initiated requests can travel together.
- Handle browser preflight and CORS headers for local browser-safe use.
- Do not let one slow product request block probes or other LocalHttp requests.
- Keep LocalHttp parsing/listening modules under the source-size rule.

## Implementation

- Added `LocalHttpProtocolHandler` for product App Server Protocol requests.
- Added `LocalHttpAppHandler` to multiplex compatibility probe requests and
  product requests on the same endpoint.
- Extended the listener to read `X-OpenAIDE-Connection-Id` and return CORS
  headers.
- Added `OPTIONS` preflight handling.
- Changed the process listener loop to accept connections and spawn one worker
  per stream with cloned handlers.
- Split HTTP parsing/writing into `listener/http.rs`.
- Reused stdio wire message serialization for LocalHttp product responses.

## Review

- Ran focused correctness review with a subagent.
- Fixed preflight/CORS compatibility finding.
- Fixed slow-request availability finding with per-connection workers.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime protocol_edge --lib`
- `cargo test -p openaide-runtime app_server_process --lib`
- `cargo test -p openaide-runtime app_server_client --lib`
- `git diff --check`
- Source-size scan for touched files.
