# P58 ACP Probe/Auth Implementation

Completed: 2026-06-27T03:55:04+03:00

## Implemented

- Added `agent/acp_probe_auth.rs`.
- Moved temporary ACP probe connection execution out of
  `agent/acp_runtime_kernel.rs`.
- Moved temporary ACP authentication connection execution, authentication host
  capability handlers, timeout wrapping, initialize/auth validation calls, and ACP
  error mapping out of `agent/acp_runtime_kernel.rs`.
- Registered the new module in `agent/mod.rs`.
- Updated ACP tests to import probe result projection and auth validation from the
  modules that own those helpers instead of through kernel/lifecycle facades.

## Ownership

- `acp_probe_auth.rs` owns temporary ACP probe/auth client construction, request
  handlers, async timeouts, initialize/auth validation calls, and ACP error mapping.
- `acp_runtime_kernel.rs` owns registry lookup, sync thread spawning, timeout receive
  wrappers, `last_agent_auth_method` cache mutation, options-session state, active
  session state, and shutdown.
- `acp_agent_status.rs` owns converting initialize responses into public probe
  results.
- `acp_session_capabilities.rs` owns initialize protocol and auth method validation.

## Behavior

No intended behavior change. Probe/auth timeout values and messages, host capability
handling, auth method validation, probe result shape, auth cache ownership,
options-session behavior, active-session behavior, shutdown, and public Agent runtime
APIs are unchanged.
