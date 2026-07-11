# ACP Session Opening Split: Integration Verification

## Result

Passed.

## Verification Commands

- `cargo test -p openaide-runtime active_session_runtime::start_prompt_and_close_dispatch_through_active_sessions -- --nocapture`
- `cargo test -p openaide-runtime active_session_runtime::load_session_registers_active_session_for_close -- --nocapture`
- `cargo test -p openaide-runtime active_session_runtime::start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`
- `cargo test -p openaide-runtime options_start_failure_reports_agent_error_instead_of_closed_start_channel -- --nocapture`
- `cargo test -p openaide-runtime prompt_content_includes_text_and_resource_links_for_path_attachments -- --nocapture`
- `cargo test -p openaide-runtime`
- `cargo fmt --all --check`
- `npm run check`
- `git diff --check`
- `jq empty .workflow/finish-refactor-plan/state.json`

## Source File Size Check

- `agent/acp_session_worker.rs`: 236 lines.
- `agent/acp_session_opening.rs`: 138 lines.

Both production source files are under the project source-file size limit.
