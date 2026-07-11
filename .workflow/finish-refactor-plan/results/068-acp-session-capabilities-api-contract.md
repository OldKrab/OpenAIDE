# P47 ACP Session Capabilities API Contract

Completed: 2026-06-27T03:30:28+03:00

## Accepted Shape

Add a focused internal module:

- `agent/acp_session_capabilities.rs`

It owns ACP initialize/capability/auth helper behavior:

- `initialize_supports_session_close(initialize)`
- `initialize_supports_session_delete(initialize)`
- `validate_initialize_protocol(initialize)`
- `validate_auth_method(initialize, method_id)`
- `auth_method_kind(method)`
- `validate_session_list_capability(initialize)`
- `validate_load_session_capability(initialize)`
- auth retry method selection for session new/load/list flows

`agent/acp_session_lifecycle.rs` keeps session new/load/list/close/delete request
behavior and may call the new capability helper module.

## Stable API

The following internal functions must remain available to existing runtime modules
and tests, either by direct import from the new module or by short compatibility
re-exports from `acp_session_lifecycle.rs` during this slice:

- `initialize_supports_session_close`
- `initialize_supports_session_delete`
- `validate_initialize_protocol`
- `validate_auth_method`
- `auth_method_kind`
- `validate_session_list_capability`
- `validate_load_session_capability`

## Ownership

- `AcpSessionCapabilities` owns pure inspection/validation of ACP initialize data and
  auth method selection policy.
- `AcpSessionLifecycle` owns side-effecting ACP session operations: new, load, list,
  close, delete, replay capture, and response normalization.

## Non-Goals

- No ACP behavior change.
- No auth retry policy change.
- No capability error message change.
- No trace behavior change.
- No list/load/start/close/delete request ordering change.
- No public Agent runtime API change.
- No test deletion or weakening.

## Review And Test Requirements

- Existing ACP capability, auth, session list, start, and load tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime` and `npm test` must pass.
- Production source files touched or added by this slice should stay below the source
  size limit where possible; if `acp_runtime_kernel.rs` or `acp_session_worker.rs`
  remain oversized, they must be explicitly treated as later split targets.
