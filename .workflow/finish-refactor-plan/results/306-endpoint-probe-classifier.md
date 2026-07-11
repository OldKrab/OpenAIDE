# 306 Endpoint Probe Classifier

## Scope

Third A7 implementation slice: add the shell-neutral endpoint probe classifier
used by future concrete local transport probes.

## Contract

- Do not implement HTTP, websocket, stdio, process launch, or shell integration.
- Classify observed endpoint/init facts into `EndpointProbeOutcome`.
- Return `EndpointProbeReport` values bound to the exact runner target and
  requirements.
- Compatible requires:
  - supported endpoint transport;
  - matching state-root fingerprint;
  - matching required protocol version;
  - matching required app version.
- State-root mismatch, protocol mismatch, app mismatch, auth failure, server
  stopping, and unreachable endpoint facts are explicit outcomes.
- Unsupported endpoint transport is collapsed to unreachable/stale for reuse;
  it is not a separate product state in this slice.

## Non-Goals

- No network client dependency.
- No App Server Protocol request implementation.
- No endpoint registration after launch.
- No shell launcher integration.

## Implementation

- Added `app_server_client::probe` with:
  - `EndpointProbeAdapter`;
  - `EndpointTransportProbe`;
  - `EndpointProbeEndpoint`;
  - `EndpointProbeObservation`;
  - `EndpointProbeFacts`;
  - `EndpointProbeLifecycle`;
  - `classify_observation`.
- The adapter implements the runner's `EndpointProber` trait.
- Transport implementations see only the selected endpoint and auth token, not
  the full endpoint target or compatibility requirements.
- Transport support is owned by the injected transport probe through
  `supports_transport`.
- The classifier binds every `EndpointProbeReport` to the original target and
  requirements.
- Unsupported endpoint transports collapse to `Unreachable` for reuse.
- Added focused tests for compatible, state-root mismatch, protocol mismatch,
  app mismatch, auth failure, unreachable, server stopping, unsupported
  transport, multi-endpoint fallback, report binding, auth-token forwarding, and
  transport probe errors.

## Review

- Correctness review reported clean.
- Module-quality review found the first probe trait was too broad, transport
  support was a caller-maintained invariant, an internal flag was pointless, and
  docs described unsupported transport inconsistently.
- Fixed by narrowing the probe input, moving support checks onto the transport
  probe, removing the pointless flag, and documenting unsupported transport as
  `Unreachable`.
- Narrow re-review found one remaining wording conflict in the plan; fixed it.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_client --lib`
- `cargo test -p openaide-runtime`
- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`
- Production Rust source-size scan under `openaide-rs/app-server/src`

## Next

Implement the concrete local transport probe that speaks the reusable App Server
Protocol initialize/health path over a browser-safe local endpoint.
