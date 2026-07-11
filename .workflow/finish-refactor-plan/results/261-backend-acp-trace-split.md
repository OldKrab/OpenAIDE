# Backend ACP Trace Split

## Contract

Split focused ACP trace helpers out of
`openaide-rs/app-server/src/agent/acp_trace.rs` while preserving
`AcpTraceState`, `AcpTraceSession`, `AcpTraceStatus`, `RuntimeSettings`, and
`RuntimeDeveloperSettings` as the stable Agent trace API.

Ownership:

- `acp_trace.rs`: public Agent trace facade and public settings/status types.
- `acp_trace/state.rs`: mutable enabled/root state, environment/default
  construction, status, and enablement updates.
- `acp_trace/session.rs`: per-session record APIs, line-direction mapping, lazy
  file opening, JSONL event construction, and serialization-failure handling.
- `acp_trace/file.rs`: trace file creation/opening, trace-opened event, write
  helper, and failure diagnostics.
- `acp_trace/naming.rs`: environment parsing, compact timestamp, and safe file
  segment construction.
- `acp_trace/tests.rs`: ACP trace unit tests.

Do not change environment variable names, default diagnostics directory,
accepted enable values, runtime settings shape, trace JSONL fields,
trace-opened event, sensitive marker, file naming sanitization/truncation,
failure logging, eprintln diagnostics, lazy file creation, ACP protocol
behavior, Agent runtime behavior, logging sanitization policy, runtime settings
protocol shape, or existing tests in this slice.

Focused tests:

- ACP trace unit tests cover enable parsing and lazy file creation.
- ACP session connection trace tests cover integration trace writes.
- Runtime settings contract tests cover live trace settings update shape.

## Implementation

Implemented the split by keeping `acp_trace.rs` as the public Agent trace
facade and moving state, session recording, trace file writing, naming/env
helpers, and tests into focused private modules.

Production source sizes after split:

- `acp_trace.rs`: 28 lines.
- `acp_trace/state.rs`: 72 lines.
- `acp_trace/session.rs`: 70 lines.
- `acp_trace/file.rs`: 69 lines.
- `acp_trace/naming.rs`: 39 lines.
- `acp_trace/tests.rs`: 39 lines.

## Review

`$doomsday-review`:

- Correctness/spec/tests: no findings.
- Code quality: local pass found no findings.

## Verification

Focused checks already run:

- `cargo fmt --all --check`: pass.
- `cargo check -p openaide-runtime`: pass.
- `cargo test -p openaide-runtime agent::acp_trace::tests -- --nocapture`: pass.
- `cargo test -p openaide-runtime agent::acp_session_connection::tests::notification_handler_traces_and_forwards_unmatched_updates_without_retry -- --nocapture`: pass.
- `cargo test -p openaide-runtime runtime_settings_patch_updates_developer_acp_trace_live -- --nocapture`: pass.
- `cargo test -p openaide-runtime agent::acp_trace -- --nocapture`: pass.
- `cargo test -p openaide-runtime agent::acp_session_connection -- --nocapture`: pass.

Final checks:

- `npm run check`: pass.
- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is
  `acp_trace/state.rs` at 72 lines.

## Commit

This commit: `refactor: split backend acp trace`.

## Next

After this slice is committed, select the next compact refactor slice from the
current plan and architecture/file-size pressure.
