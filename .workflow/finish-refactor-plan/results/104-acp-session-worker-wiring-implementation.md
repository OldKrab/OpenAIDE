# ACP Session Worker Wiring Implementation

Implemented the accepted ACP session worker client/host wiring split.

## Changes

- Added `agent/acp_session_connection.rs`.
- Moved ACP `Client` construction out of `agent/acp_session_worker.rs`.
- Kept worker lifecycle, ACP process creation, initialize/start/load, config catalog delivery, and command-loop handling in `agent/acp_session_worker.rs`.
- Centralized moved connection behavior in `connect_acp_session_client`:
  - client name wiring;
  - `session/update` trace recording;
  - load-replay capture by matching session id;
  - unhandled notification fallback with no retry;
  - `AcpHostCapabilityHandlers` construction;
  - Agent-initiated permission, filesystem, and terminal request handlers.
- Added `agent/acp_session_connection/tests.rs` for connection-level regression coverage without growing production source files.

## Boundary Result

The worker now depends on one worker-facing connection helper and no longer owns the ACP client builder registration chain. The extracted module owns only connection and host-capability wiring, not session lifecycle decisions.
