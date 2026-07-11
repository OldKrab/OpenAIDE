# P76 Next Slice Selection

Completed: 2026-06-27T04:45:52+03:00

## Selected Slice

Split active ACP session registry and worker spawning out of
`agent/acp_runtime_kernel.rs`.

## Rationale

`agent/acp_runtime_kernel.rs` remains one of the largest production files at 387
lines. Its largest cohesive remaining responsibility is active ACP task-session
management:

- start/load worker spawning and startup timeout handling;
- active session registry ownership;
- duplicate active session protection;
- resume check against the active registry;
- event-sink attachment;
- prompt/cancel/close/delete dispatch to active sessions;
- shutdown close-task extraction for active sessions.

This stateful active-session manager is distinct from:

- `acp_session_worker.rs`, which owns the live worker loop;
- `acp_session_client.rs`, which owns the command client for an already-started
  session;
- `acp_options_session_manager.rs`, which owns prepared options-session lifecycle;
- probe/auth execution, which is already split out.

Extracting the active-session manager moves `AcpRuntimeKernel` closer to a facade
over public Agent runtime operations and leaves probe/auth, options sessions, and
active task sessions behind separate internal boundaries.

## Non-Selection

Do not change ACP worker behavior, prompt behavior, cancel/close/delete command
behavior, startup timeout values or error text, trace behavior, auth-method cache
semantics, duplicate session behavior, or shutdown close ordering.

Do not move probe/auth execution, options-session lifecycle, live worker loop,
prompt runner, or ACP session client internals in this slice.
