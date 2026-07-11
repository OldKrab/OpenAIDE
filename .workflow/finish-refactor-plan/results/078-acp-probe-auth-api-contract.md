# P57 ACP Probe/Auth API Contract

Completed: 2026-06-27T03:46:18+03:00

## Accepted Shape

Add a focused internal module:

- `agent/acp_probe_auth.rs`

It owns temporary ACP connection execution for Agent setup utility operations:

- probe connection execution;
- authentication connection execution;
- ACP initialize protocol validation for those temporary connections;
- host capability handlers needed during authentication;
- probe/auth timeout wrapping and ACP error mapping.

`agent/acp_runtime_kernel.rs` keeps the public runtime facade methods and stateful
kernel policy:

- registry lookup;
- public `probe` and `probe_with_timeout` methods;
- public `authenticate` method;
- `last_agent_auth_method` cache update after successful authentication;
- active session registry and session worker spawning;
- options-session lifecycle and retry;
- shutdown.

## Stable API

The new module should expose only narrow internal functions, with call shapes close to:

- `run_probe_with_timeout(config, agent_id, timeout, host_bridge)`
- `run_authenticate_with_timeout(config, request, timeout, host_bridge)`

The functions return existing protocol model results:

- `AgentProbeResult`
- `AgentAuthenticateResult`

`AcpRuntimeKernel` remains responsible for validating an empty `method_id` before
auth execution and storing `last_agent_auth_method` after success.

## Ownership

- `acp_probe_auth.rs` owns temporary ACP probe/auth client construction, request
  handlers, async timeout, initialize/auth validation calls, and `acp_error` mapping.
- `acp_runtime_kernel.rs` owns stateful runtime orchestration around those calls,
  including registry access, thread spawning if still needed by the sync facade,
  auth cache mutation, options-session state, active-session state, and shutdown.
- `acp_agent_status.rs` remains the owner of converting initialize responses into
  public probe results.
- `acp_session_capabilities.rs` remains the owner of protocol and auth method
  validation predicates.

## Non-Goals

- No ACP behavior change.
- No timeout value or timeout error text change.
- No auth method validation behavior change.
- No host capability behavior change during authentication.
- No probe result shape change.
- No `last_agent_auth_method` cache ownership change.
- No options-session, active-session, shutdown, or session worker spawning change.
- No public Agent runtime API change.
- No test deletion or weakening.

## Review And Test Requirements

- Existing probe/auth ACP runtime tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime`, `npm run check`, and `npm test` must pass before
  commit.
- Touched production source files should stay below the source-size limit where
  possible. `acp_runtime_kernel.rs` may remain oversized after this slice, but the
  next oversized responsibility must be explicit in the workflow state.
