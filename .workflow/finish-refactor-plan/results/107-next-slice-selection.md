# Next Slice Selection: ACP Session Request I/O Split

## Selected Slice

Split raw ACP session request I/O helpers out of
`agent/acp_session_lifecycle.rs` into a focused module, tentatively
`agent/acp_session_requests.rs`.

## Why This Slice

`agent/acp_session_lifecycle.rs` now mixes two responsibilities:

- lifecycle orchestration for starting, loading, listing, closing, deleting, and
  attaching ACP sessions;
- raw ACP request construction, retry-after-auth behavior, and trace recording
  for `session/new`, `session/load`, and `session/list`.

The lifecycle module is still under the production source-size limit, but its
boundary is broad enough that the next clean refactor should separate ACP
request I/O from lifecycle decisions before adding more Agent/session behavior.

## Intended Boundary

The new request module should own:

- `session/new` request construction and trace recording;
- `session/load` request construction, trace recording, and auth-required retry;
- `session/list` request construction and auth-required retry;
- ACP authentication retry helper calls needed by those requests.

`agent/acp_session_lifecycle.rs` should keep:

- `LoadReplayCapture`;
- start/load orchestration;
- active session attachment;
- replay projection into normalized Chat;
- session-list result normalization and filtering;
- close/delete lifecycle helpers unless a later contract explicitly moves them.

## Constraints

- No behavior changes.
- Keep current ACP auth retry semantics unchanged.
- Keep trace event names and directions unchanged.
- Keep public `pub(super)` call sites stable unless the accepted API contract says
  otherwise.
- Do not move replay capture or replay projection in this slice.
- Do not move close/delete helpers unless the next API grill accepts that
  explicitly.

## Next Step

Grill and record the API contract for the ACP session request I/O split.
