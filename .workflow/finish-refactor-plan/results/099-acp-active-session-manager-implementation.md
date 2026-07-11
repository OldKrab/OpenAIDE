# ACP Active-Session Manager Implementation

Date: 2026-06-27

## Scope

Implemented the accepted ACP active-session manager split.

## Changes

- Added `agent/acp_active_session_manager.rs`.
- Moved active ACP task-session registry ownership out of `agent/acp_runtime_kernel.rs`.
- Moved start/load worker spawning, startup timeout handling, duplicate active-session protection, prompt/cancel/close/delete dispatch, resume, event-sink attach, and shutdown close-task extraction into the active-session manager.
- Kept `agent/acp_runtime_kernel.rs` as the public ACP runtime coordinator for registry, host bridge, options sessions, probe/auth, auth-method cache, and active-session delegation.
- Moved session startup worker contracts from `agent/acp_session_client.rs` into `agent/acp_session_worker.rs`, leaving the client module focused on already-started worker commands and terminal error reporting.
- Preserved construction-time trace configuration without runtime trace-state locking.

## Tests Added

- Added `agent/acp/tests/active_session_runtime.rs` as a focused runtime-boundary test module.
- Covered start/prompt/close, resume, attach event sink, cancel during active prompt, duplicate active-session cleanup, load registration, delete dispatch, shutdown close extraction, startup timeout mapping, and startup failure error reporting.
- Guarded external fixture dependencies so missing `python3` does not fail otherwise valid Rust test environments.
