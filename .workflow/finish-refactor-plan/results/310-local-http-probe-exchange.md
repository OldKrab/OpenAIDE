# 310 LocalHttp Probe Exchange

## Scope

Seventh A7 implementation slice: add a concrete `LocalHttp` exchange for the
shared `client/probe` JSON-RPC adapter.

## Contract

- Support only `TransportKind::LocalHttp`.
- Send a single HTTP POST containing the JSON-RPC request body.
- Forward the endpoint auth token as local transport access control.
- Map 200 responses to parsed JSON.
- Map 401/403 to `AuthFailed`.
- Map connection refusal or timeout to `Unreachable`.
- Treat malformed addresses, malformed HTTP responses, and invalid JSON bodies
  as hard probe errors.

## Non-Goals

- No App Server HTTP listener.
- No shell launcher integration.
- No endpoint record writer.
- No normal product traffic transport.

## Next

Commit this slice, then design the matching local App Server HTTP listener or
the next attach-or-launch integration point that can consume this exchange.

## Implementation

- Added `ClientProbeExchange` implementation for `TransportKind::LocalHttp`.
- Sends a single HTTP/1.1 POST with JSON request body.
- Sends the process token only after validating the endpoint is loopback.
- Parses HTTP responses by headers and `Content-Length`; it does not depend on
  EOF-delimited bodies.
- Maps:
  - HTTP 200 to parsed JSON;
  - HTTP 401/403 to `AuthFailed`;
  - connection refusal, read timeout, write timeout, and `WouldBlock` timeout
    behavior to `Unreachable`;
  - malformed endpoint address, HTTP response, or JSON body to probe errors.
- Added loopback `TcpListener` tests for success, auth failure, unreachable,
  non-loopback rejection, stalled response timeout, and malformed responses.

## Review

- First independent review found three correctness issues:
  - non-loopback endpoints could receive bearer tokens;
  - success parsing waited for EOF instead of honoring `Content-Length`;
  - accepted-then-stalled timeouts could become hard probe errors.
- Fixes:
  - reject non-loopback socket addresses before constructing/sending the
    Authorization header;
  - parse response headers and read the declared body length;
  - map `TimedOut` and `WouldBlock` read/write failures to `Unreachable`.
- Follow-up independent review: clean.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_client::probe::exchange::local_http --lib`
- `cargo test -p openaide-runtime app_server_client::probe::exchange --lib`
- `cargo test -p openaide-runtime app_server_client --lib`
- `jq empty .workflow/finish-refactor-plan/state.json && git diff --check`
- Source-size scan for `app_server_client` production files; new
  `local_http.rs` is 298 lines and must be split before further growth.
