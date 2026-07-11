# P22 ACP Config Options Apply API Contract

Completed: 2026-06-27T02:47:20+03:00

## Accepted Shape

Add a focused internal module:

- `agent/acp_config_options_apply.rs`

It owns:

- `apply_config_options(...)`
- `set_prepared_config_option_after_prior_updates(...)`
- `PreparedOptionsSetContext`
- private helpers for set-option request dispatch, task-start option application, and
  config selection parsing.

## Ownership

- `AcpRuntimeKernel` keeps ownership of runtime registry state, options session
  creation/invalidation, active ACP sessions, cancellation, close/delete, prompt
  routing, probe, and authentication.
- `acp_options_session` keeps ownership of the long-lived prepared options session
  worker.
- `acp_config_options_apply` owns only applying selected config options against an
  existing ACP connection/active session and preserving update-order semantics.

## Non-Goals

- No ACP behavior change.
- No timeout change.
- No protocol mapping change.
- No options cache.
- No movement of session start/load/resume/close lifecycle.
- No test deletion or weakening.

## Review And Test Requirements

- Existing ACP config-option tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime` and `npm test` must pass.
- `agent/acp_runtime_kernel.rs` should become smaller without introducing a new
  oversized production module.
