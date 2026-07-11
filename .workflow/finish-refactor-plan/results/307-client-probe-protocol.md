# 307 Client Probe Protocol

## Scope

Fourth A7 implementation slice: add the pre-initialize App Server Protocol
probe method used by attach-or-launch endpoint validation.

## Contract

- Add typed `client/probe` request/response definitions.
- Allow `client/probe` before `client/initialize`.
- Do not register, reattach, initialize, or disconnect an App Shell client.
- Return endpoint validation facts:
  - state-root fingerprint;
  - protocol version;
  - app version;
  - lifecycle.
- Keep transport authentication outside this method.
- Generate TypeScript bindings and method maps for the new method.

## Non-Goals

- No HTTP, websocket, or stdio transport probe implementation.
- No endpoint record registration.
- No shell launcher integration.
- No product snapshot or subscription state in the probe response.

## Next

Commit this slice, then implement the attach-or-launch transport prober that
calls `client/probe` and feeds the typed probe facts into the existing endpoint
classification API.

## Implementation

- Added typed `ClientProbeParams`, `ClientProbeResult`, and
  `ClientProbeLifecycle` protocol records.
- Added `client/probe` to protocol method constants, method maps, and generated
  TypeScript bindings.
- Added protocol-edge `AppServerProbeFacts` and a pre-initialize
  `client/probe` handler.
- Allowed only `client/probe` to bypass the initialize gate; product requests
  remain gated by `client/initialize`.
- Wired production stdio gateway construction to provide state-root,
  protocol-version, and app-version probe facts.
- Split focused `client/probe` tests into a sibling Rust test module.

## Review

- First independent review: clean.
- Second independent review found stale ignored `dist` output and missing
  `draining` lifecycle coverage.
- Fixes:
  - rebuilt `@openaide/app-server-client` so ignored `dist` output is current on
    disk;
  - added direct `draining` probe lifecycle coverage.
- Follow-up review was requested after fixes.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-app-server-protocol --lib`
- `cargo test -p openaide-runtime protocol_edge --lib`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`
- `npm run check --workspace @openaide/app-server-client`
- `jq empty .workflow/finish-refactor-plan/state.json && git diff --check`
- Production source-size scan; touched production files remain under the
  400-line limit.
