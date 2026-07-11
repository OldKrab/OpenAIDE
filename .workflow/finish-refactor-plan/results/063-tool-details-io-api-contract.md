# P42 Tool Details I/O API Contract

Completed: 2026-06-27T03:22:22+03:00

## Accepted Shape

Add a focused internal helper module:

- `agent/tool_details_io.rs`

Keep `agent/tool_details.rs` as the public facade for tool-call event projection.

## Stable API

No caller-facing API changes:

- `tool_call_event(tool_call)`
- `tool_kind_name(kind)`

The new helper module exposes only crate-internal helpers needed by
`tool_details.rs`, such as:

- `tool_input_summary(raw_input)`
- `tool_input_detail(value)`
- `tool_output_detail(value)`

Exact helper names may vary, but the helper module owns raw input/output detail
projection and sanitization.

## Ownership

- `tool_details.rs` owns the top-level `ToolCall` to `AgentEvent` projection, tool
  kind/status mapping, output content preview, content detail mapping, and location
  detail mapping.
- `tool_details_io.rs` owns raw input/output conversion, scalar field summarization,
  path leaf summaries, command summaries, sensitive key redaction, and preview
  truncation used for raw input/output.

## Non-Goals

- No Agent event shape change.
- No `ActivityToolDetails`, `ActivityToolInput`, or `ActivityToolOutput` shape change.
- No redaction behavior change.
- No path/command summarization behavior change.
- No ACP behavior or lifecycle change.
- No public Agent runtime API change.

## Review And Test Requirements

- Existing ACP tool detail tests must keep passing.
- `cargo test -p openaide-runtime agent::acp::tests -- --nocapture` must pass.
- `cargo test -p openaide-runtime` and `npm test` must pass.
- Production source files touched or added by this slice should stay below the source
  size limit.
