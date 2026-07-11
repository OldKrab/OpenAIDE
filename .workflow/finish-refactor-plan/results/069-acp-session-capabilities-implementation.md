# P48 ACP Session Capabilities Implementation

Completed: 2026-06-27T03:36:48+03:00

## Implemented

- Added `agent/acp_session_capabilities.rs`.
- Moved ACP initialize capability predicates, protocol validation, auth method
  validation, session list/load capability validation, and auth retry selection out
  of `agent/acp_session_lifecycle.rs`.
- Registered the new module in `agent/mod.rs`.
- Kept stable internal helper names available to existing ACP callers through the
  lifecycle module.

## Ownership

- `acp_session_capabilities.rs` owns pure capability and auth helper logic derived
  from ACP initialize state.
- `acp_session_lifecycle.rs` owns side-effecting session new/load/list/close/delete
  operations, replay capture, request dispatch, and response normalization.

## Behavior

No intended behavior change. ACP protocol validation, auth retry method selection,
capability error messages, session lifecycle request order, tracing, and public Agent
runtime APIs are unchanged.
