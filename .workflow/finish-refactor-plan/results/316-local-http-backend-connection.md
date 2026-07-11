Packet ID: P316-local-http-backend-connection
Status: completed

Objective:
Add the shell-consumable TypeScript LocalHttp BackendConnection adapter for the
reusable App Server endpoint, and tighten the backend LocalHttp client-response
boundary needed for bidirectional server requests.

Changes:
- Added `createLocalHttpBackendConnection` in `@openaide/app-server-client`.
- Added LocalHttp wire parsing for responses, events, and Backend-initiated
  server requests.
- Added an explicit `BackendConnection.serverRequests()` channel so
  Backend-initiated requests are not mixed with state events or silently
  dropped.
- Reused the stdio JSON-RPC client-response parser for LocalHttp and tightened
  it to require JSON-RPC 2.0 plus exactly one of `result` or `error`.
- Exported the LocalHttp adapter from `@openaide/app-server-client`.
- Updated the refactor plan with the completed slice contract and next A7 step.

Review:
- Ran a bounded doomsday-style subagent review.
- Fixed the reported dropped Backend-initiated LocalHttp requests by adding the
  explicit server-request channel and tests.
- Fixed the reported loose client-response validation by sharing and tightening
  the parser and adding malformed LocalHttp response tests.

Verification:
- `cargo fmt --all --check`
- `cargo test -p openaide-runtime protocol_edge --lib`
- `npm run test --workspace @openaide/app-server-client`
- `npm run check --workspace @openaide/app-server-client`
- `npm run check`
- Production source file sizes remain below the project limit.

Next:
Wire shell bootstrap to produce ephemeral LocalHttp connection info from the
shared attach-or-launch handoff, while keeping endpoint discovery and launch
policy out of the browser Frontend.
