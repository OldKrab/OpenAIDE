# P62 ACP Options Session Manager API Contract

Completed: 2026-06-27T03:57:06+03:00

## Accepted Shape

Add focused internal modules:

- `agent/acp_options_session_manager.rs`
- `agent/acp_runtime_threading.rs`

`acp_options_session_manager.rs` owns stateful options-session lifecycle and retry:

- active `AcpOptionsSessionEntry` storage;
- options generation assignment;
- cache matching by normalized cwd and Agent options request key;
- stale active options-session close and replacement;
- retry once after `RuntimeError::NotReady`;
- invalidation by generation;
- worker thread spawning and startup receive timeout handling;
- extraction of an options-session shutdown close task.

`acp_runtime_threading.rs` owns the small shared helper for running an async ACP worker
future on a fresh Tokio runtime from a spawned thread.

`agent/acp_options_session.rs` keeps the live options worker/client protocol:

- `AcpOptionsSessionClient`;
- `AcpOptionsCommand`;
- `AcpOptionsSessionWorkerInput`;
- `run_options_session`;
- worker command loop and ACP update processing.

`agent/acp_runtime_kernel.rs` keeps public runtime facade methods and non-options
state:

- registry and host/notifier ownership;
- public `list_sessions`, `config_options`, and `set_config_option` methods that
  delegate to the options manager after request validation;
- `last_agent_auth_method` cache ownership and updates;
- active Task session registry, worker spawning, prompt/cancel/close/delete, and
  shutdown coordination.

## Stable API

The new manager exposes narrow internal methods close to:

- `AcpOptionsSessionManager::new(registry, host_bridge, notifier, last_agent_auth_method)`
- `with_options_session(agent_id, cwd, operation)`
- `take_shutdown_close_task()`

`with_options_session` accepts a closure over `AcpOptionsSessionClient`, preserving the
current kernel call shape for:

- `config_options()`;
- `set_config_option(config_id, value)`;
- `list_sessions(agent_id, cwd, cursor)`.

## Ownership

- `acp_options_session_manager.rs` owns when an options session is reused, closed,
  invalidated, retried, or spawned.
- `acp_options_session.rs` owns how a live options session serves commands once it
  exists.
- `acp_runtime_kernel.rs` owns public validation before delegation and global
  shutdown coordination across options sessions and active Task sessions.
- `acp_runtime_threading.rs` owns only the runtime bridge helper; it must not own ACP
  product behavior.

## Non-Goals

- No ACP behavior change.
- No options-session retry policy change.
- No startup timeout value or error text change.
- No list/config/set result shape change.
- No auth method cache ownership or value semantics change.
- No active Task session behavior change.
- No shutdown close ordering change beyond keeping options and active sessions in the
  same parallel close task set.
- No public Agent runtime API change.
- No test deletion or weakening.

## Review And Test Requirements

- Existing ACP options/config/session-list tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime`, `npm run check`, and `npm test` must pass before
  commit.
- `acp_runtime_kernel.rs` should move materially closer to the production source-size
  limit. If it remains above the limit, the next oversized responsibility must be
  explicit in workflow state.
