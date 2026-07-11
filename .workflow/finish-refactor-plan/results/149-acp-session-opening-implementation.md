# ACP Session Opening Split: Implementation

## Scope Implemented

Implemented the accepted ACP Session opening split.

## Code Changes

- Added `agent/acp_session_opening.rs`.
- Moved ACP session opening responsibilities out of
  `agent/acp_session_worker.rs`:
  - initialize request creation and trace recording;
  - `initialize_agent_connection`;
  - `AcpSessionRunner` construction;
  - session close capability discovery;
  - prompt content policy derivation;
  - start prompt attachment validation;
  - `session/new` start flow;
  - initial config-option application;
  - close-on-option-apply-failure;
  - `session/load` load flow and replay capture;
  - startup error reporting;
  - opened-session result construction.
- Kept `agent/acp_session_worker.rs` responsible for ACP client connection,
  startup success reporting, live command/update loop, prompt dispatch,
  close/delete command handling, session config catalog delivery, and final
  `acp_error` mapping.

## Behavior Notes

- Preserved existing duplicate initialize request tracing around
  `initialize_agent_connection`; this slice is a behavior-preserving split.
- Kept opening APIs private to Agent ACP internals.
- No new runtime behavior was added.

## Verification Before Review

- `cargo test -p openaide-runtime active_session_runtime::start_prompt_and_close_dispatch_through_active_sessions -- --nocapture`
- `cargo test -p openaide-runtime active_session_runtime::load_session_registers_active_session_for_close -- --nocapture`
- `cargo test -p openaide-runtime options_start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`
- `cargo test -p openaide-runtime active_session_runtime::start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`
- `cargo test -p openaide-runtime prompt_content_includes_text_and_resource_links_for_path_attachments -- --nocapture`
- `cargo test -p openaide-runtime`
- `cargo fmt --all --check`
- `npm run check`
- `git diff --check`

All checks passed before review.
