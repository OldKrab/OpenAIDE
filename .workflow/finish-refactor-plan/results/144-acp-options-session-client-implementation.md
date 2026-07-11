# ACP Options Session Client Split: Implementation

## Implemented

Created `agent/acp_options_session_client.rs` for the channel-facing prepared
options-session API:

- `AcpOptionsSessionClient`;
- `AcpOptionsCommandReceiver`;
- `AcpOptionsCommand`;
- `options_session_channel`;
- synchronous client methods and stopped-worker/timeout error mapping.

Kept `agent/acp_options_session.rs` responsible for the live ACP options worker:

- `AcpOptionsSessionWorkerInput`;
- `run_options_session`;
- ACP connection startup;
- permission invalidation;
- catalog projection and session update handling;
- command execution;
- set-option application;
- list-session and close execution through `AcpSessionRunner`.

`AcpOptionsSessionManager` changed only by import path.

## Verification

Passed:

- `cargo test -p openaide-runtime options_session_update -- --nocapture`
- `cargo test -p openaide-runtime options_connection_list_excludes_prepared_session -- --nocapture`
- `cargo test -p openaide-runtime options_start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`
- `cargo test -p openaide-runtime`

Formatting was applied with `cargo fmt --all`.

## Next Step

Run `$doomsday-review` on this slice and fix material findings before final
integration verification.
