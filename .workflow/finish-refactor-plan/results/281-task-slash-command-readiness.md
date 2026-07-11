# A3j Task Slash-Command Readiness

Implemented slash-command readiness as App Server-owned Task metadata.

## Implementation

- Added normalized Agent command catalog types and ACP projection for
  `available_commands_update`.
- Added idle-session catalog buffering so commands emitted before a session sink
  attaches are delivered when the sink becomes available.
- Persisted command catalogs from prepared sessions, active prompt updates, and
  loaded/adopted sessions.
- Projected command readiness into Task snapshots as loading, ready,
  unavailable, or failed state.
- Kept slash commands as composer text: command execution still goes through
  `task/send`.
- Tolerated attach-time command metadata revision bumps during `task/send`
  without accepting real concurrent task changes.

## Review

- Ran bounded doomsday review with subagent `Avicenna the 4th`.
- Fixed both important findings:
  - first send no longer rejects itself when attach emits command metadata;
  - loaded/adopted sessions preserve replayed command readiness.
- Split ACP idle-session catalog handling into `agent/acp_session_catalogs.rs`
  to keep production modules under the source-size rule.

## Verification

- `cargo test -p openaide-runtime`
  - 239 runtime unit tests passed.
  - 47 runtime-contract tests passed.

## Next

A3 is complete. Continue with A4: wire `server_requests` into permissions and
shell capabilities.
