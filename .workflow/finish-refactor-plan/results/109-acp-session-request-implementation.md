# ACP Session Request I/O Split Implementation

Implemented the accepted ACP session request I/O split.

## Changes

- Added `agent/acp_session_requests.rs`.
- Moved raw ACP request construction and blocking sends for:
  - `session/new`;
  - `session/load`;
  - `session/list`.
- Moved AuthRequired retry handling for those requests into the request module.
- Preserved existing `session/new` and `session/load` trace event names,
  directions, and payloads.
- Preserved `session/list` as untraced, matching prior behavior.
- Kept `agent/acp_session_lifecycle.rs` responsible for lifecycle orchestration,
  capability validation, session attachment, replay capture/projection,
  config-option normalization, listed-session filtering, close/delete helpers,
  and product error mapping.
- Kept `request_session_list` re-exported from lifecycle for existing sibling
  tests and call paths.

## Boundary Result

`agent/acp_session_lifecycle.rs` now reads as lifecycle orchestration over
request helpers. `agent/acp_session_requests.rs` reads as raw ACP request I/O
with retry and tracing only.
