# P46 Next Slice Selection

Completed: 2026-06-27T03:30:28+03:00

## Selected Slice

Split ACP session capability and authentication helper logic out of
`agent/acp_session_lifecycle.rs`.

## Rationale

`agent/acp_session_lifecycle.rs` is oversized, but its highest-risk code is session
start/load/close/delete behavior. The capability and authentication helpers are a
lower-risk, cohesive boundary:

- protocol version validation;
- auth method kind/validation;
- single-agent-auth retry selection;
- session close/delete support predicates;
- list/load capability validation.

Moving those helpers first reduces lifecycle module size without changing live session
startup, replay capture, close, delete, or list request behavior.

## Non-Selection

Do not move session start/load/close/delete behavior in this slice.

Do not change auth retry policy, capability semantics, error messages, tracing, or ACP
request ordering.
