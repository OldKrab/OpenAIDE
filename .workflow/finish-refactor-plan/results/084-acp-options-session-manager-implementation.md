# ACP Options Session Manager Implementation

## Scope

Implemented the accepted contract in `083-acp-options-session-manager-api-contract.md`.

## Code Changes

- Added `agent/acp_options_session_manager.rs` for ACP options-session lifecycle:
  active options-session reuse, request-key matching, generation invalidation,
  retry-on-`NotReady`, worker startup timeout handling, and shutdown close-task
  extraction.
- Added `agent/acp_runtime_threading.rs` for generic runtime/threading helpers:
  `block_on_new_runtime` and `close_in_parallel`.
- Added `agent/acp_session_paths.rs` for shared session cwd normalization.
- Added `agent/acp_auth_method_cache.rs` to keep the auth-method cache behind a
  typed boundary instead of sharing a raw mutex between modules.
- Kept `agent/acp_options_session.rs` focused on the live prepared-options worker
  protocol. It now owns the command enum and exposes only a client plus opaque
  receiver factory.
- Reduced `agent/acp_runtime_kernel.rs` to public request validation/unpacking,
  active ACP task sessions, probe/auth orchestration, and top-level shutdown
  coordination.

## Behavior Contract

- No ACP request, response, timeout value, error text, retry policy, or public
  runtime API behavior was intentionally changed.
- Options-session retry remains exactly one recreate-and-retry after
  `RuntimeError::NotReady`.
- Options and active session close tasks remain in the same parallel shutdown
  task set.
- Authentication still records the last authenticated method once authenticate
  succeeds, and new ACP sessions/options sessions read that preferred method.
