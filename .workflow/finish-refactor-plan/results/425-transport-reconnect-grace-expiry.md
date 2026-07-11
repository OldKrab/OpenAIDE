# P425 Transport Reconnect Grace Expiry

## Status

Completed.

## Implementation

Added heartbeat-driven client liveness for LocalHttp App Server clients.

Backend:

- Added typed `client/heartbeat` protocol records.
- Added generated TypeScript binding support for heartbeat params/results.
- `ClientHub` now tracks liveness deadlines per initialized client.
- Normal initialized product requests refresh liveness.
- `RpcGateway::expire_inactive_clients` expires inactive clients, interrupts
  client-scoped server requests, and transitions lifecycle to draining when the
  last initialized client expires.
- LocalHttp uses wall-clock `AppServerTime`.
- App Server LocalHttp endpoint publication starts a background liveness
  expiry loop.

Frontend client:

- `createLocalHttpBackendConnection` starts heartbeat after successful
  initialize and clears it on close.
- Heartbeat errors are swallowed because normal user-facing requests own
  transport error presentation.

## Verification

Passed:

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime -p openaide-app-server-protocol`
- `cargo test -p openaide-runtime client_lifecycle::tests`
- `cargo test -p openaide-runtime protocol_edge::tests::heartbeat_refreshes_client_liveness`
- `cargo test -p openaide-runtime protocol_edge::tests::inactive_expiry_interrupts_client_scoped_requests`
- `cargo test -p openaide-app-server-protocol methods::tests`
- `cargo test -p openaide-app-server-protocol typescript::tests`
- `npm run protocol:check`
- `npm run test --workspace @openaide/app-server-client -- localHttpConnection`
- `npm run build --workspace @openaide/app-server-client`
- `git diff --check`
- Rust and App Server Client production source-size guards

## Next Packet

P426 should wire clean shutdown completion:

- remove endpoint records according to the shutdown plan;
- mark storage clean only after coherent shutdown work succeeds;
- keep unclean marker behavior when shutdown cannot persist coherent state.
