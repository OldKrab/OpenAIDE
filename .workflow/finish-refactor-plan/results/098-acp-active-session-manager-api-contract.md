# P77 ACP Active Session Manager API Contract

Completed: 2026-06-27T04:45:52+03:00

## Accepted Shape

Add `agent/acp_active_session_manager.rs` as a focused internal module.

`acp_active_session_manager.rs` owns active ACP task-session lifecycle:

- active `HashMap<String, AcpSessionClient>` registry;
- `start_session`;
- `load_session`;
- `resume_session`;
- `attach_session_event_sink`;
- `prompt`;
- `cancel_session`;
- `close_session`;
- `delete_session`;
- active-session worker spawning and startup timeout handling;
- duplicate active session rejection and cleanup;
- active-session shutdown close-task extraction.

`agent/acp_runtime_kernel.rs` remains the public runtime operation coordinator:

- registry, host bridge, trace state, auth-method cache, and options-session manager
  construction;
- public probe/auth/options/list methods;
- delegation to the active-session manager for active task-session operations;
- top-level shutdown coordination across options sessions and active sessions.

## Stable API

The active manager exposes narrow internal methods close to:

- `AcpActiveSessionManager::new(registry, host_bridge, auth_method_cache)`
- `with_trace_state(trace_state)`
- `start_session(request) -> AgentSession`
- `load_session(request) -> AgentLoadedSession`
- `resume_session(request) -> AgentSession`
- `attach_session_event_sink(session_id, sink)`
- `prompt(prompt, sink)`
- `cancel_session(session_id)`
- `close_session(session_id)`
- `delete_session(request)`
- `take_shutdown_close_tasks() -> Vec<Box<dyn FnOnce() + Send + 'static>>`

`AcpRuntimeKernel` keeps the existing public method names and result shapes and
delegates active-session operations to the manager.

## Ownership

- `acp_active_session_manager.rs` owns active session registry state and worker spawn
  setup for start/load.
- `acp_session_worker.rs` owns live worker I/O and command handling after a worker
  starts.
- `acp_session_client.rs` owns sending commands to an already-started worker.
- `acp_runtime_kernel.rs` owns cross-manager orchestration and public runtime facade
  methods.

## Non-Goals

- No public Agent runtime API change.
- No ACP behavior change.
- No start/load/resume behavior change.
- No prompt/cancel/close/delete behavior change.
- No startup timeout value or error text change.
- No duplicate active session behavior change.
- No trace session behavior change.
- No auth-method cache ownership or value semantic change.
- No shutdown close ordering change beyond preserving options-session close tasks and
  active-session close tasks in the same parallel close task set.
- No test deletion or weakening.

## Review And Test Requirements

- Existing ACP session lifecycle tests must keep passing.
- Existing prompt/cancel/close/delete tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime`, `npm run check`, and `npm test` must pass before
  commit.
- All touched production source files must remain below the 400-line limit.
