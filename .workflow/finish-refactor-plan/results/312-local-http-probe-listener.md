# 312 LocalHttp Probe Listener

## Scope

Ninth A7 implementation slice: add probe-only LocalHttp socket handling for
`client/probe`.

## Contract

- Accept one HTTP request per connection.
- Support POST with `Content-Length`.
- Extract Authorization and body, then delegate to `LocalHttpProbeHandler`.
- Write status, content length, and JSON content type when a body exists.
- Return local HTTP errors for malformed framing without calling protocol code.
- Do not publish endpoint records, launch processes, or support normal product
  traffic.

## Non-Goals

- No endpoint record writer.
- No process launcher.
- No full App Server HTTP product transport.
- No shell integration.

## Next

Commit this slice, then batch the next A7 work: wire the listener into startup,
publish LocalHttp endpoint records, and make attach-or-launch use the concrete
LocalHttp probe exchange.

## Implementation

- Added probe-only `LocalHttpProbeListener`.
- Added one-request-per-connection HTTP handling.
- Supports POST plus `Content-Length`.
- Extracts Authorization and body and delegates to `LocalHttpProbeHandler`.
- Returns local HTTP 400/405 without calling protocol code for malformed or
  unsupported requests.
- Writes status, content length, connection close, and JSON content type when a
  body exists.

## Review

- Skipped independent review for this helper-only slice to speed up A7; next
  integrated listener/endpoint/runner batch must get review.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime protocol_edge::local_http::listener --lib`
- `cargo test -p openaide-runtime protocol_edge --lib`
- `jq empty .workflow/finish-refactor-plan/state.json && git diff --check`
- Protocol-edge production source-size scan.
