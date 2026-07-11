# 308 Probe Result Mapping

## Scope

Fifth A7 implementation slice: connect the typed `client/probe` protocol result
to the shared attach-or-launch probe classifier.

## Contract

- Convert `ClientProbeResult` into `EndpointProbeFacts`.
- Preserve state-root fingerprint, protocol version, app version, and lifecycle.
- Treat `running` and `draining` as reusable endpoint lifecycles.
- Treat `stopping` as `ServerStopping`.
- Do not add real transport, shell, launch, or endpoint registration code.

## Non-Goals

- No HTTP, websocket, or stdio client implementation.
- No browser or shell bootstrap integration.
- No endpoint record writer.

## Next

Commit this slice, then implement the first real transport-specific
`EndpointTransportProbe` that sends typed `client/probe` requests to reusable
App Server endpoints.

## Implementation

- Added `From<ClientProbeResult> for EndpointProbeFacts`.
- Added local `EndpointProbeLifecycle::Draining`.
- Added `From<ClientProbeLifecycle> for EndpointProbeLifecycle`.
- Left classifier behavior narrow:
  - `running` and `draining` are compatible when root/protocol/app match;
  - `stopping` maps to `ServerStopping`.

## Review

- Independent review found a low-risk test gap: protocol lifecycle mapping and
  classification were not covered end to end for all lifecycle values.
- Fixed with a table-driven test converting each `ClientProbeLifecycle` through
  `ClientProbeResult -> EndpointProbeFacts -> classify_observation`.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime app_server_client::probe --lib`
- `cargo test -p openaide-runtime app_server_client --lib`
- `jq empty .workflow/finish-refactor-plan/state.json && git diff --check`
- Source-size scan for `app_server_client` production files.
