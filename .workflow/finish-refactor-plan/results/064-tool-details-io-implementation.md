# P43 Tool Details I/O Implementation

Completed: 2026-06-27T03:28:25+03:00

## Implemented

- Added `agent/tool_details_io.rs`.
- Moved raw tool input/output detail projection and sanitization helpers out of
  `agent/tool_details.rs`.
- Kept `agent/tool_details.rs` as the top-level ToolCall to AgentEvent projection
  facade.

## Ownership

- `tool_details.rs` owns `tool_call_event`, `tool_kind_name`, tool status mapping,
  content preview, content details, and locations.
- `tool_details_io.rs` owns raw input/output details, command/path/scalar summaries,
  sensitive key redaction, and preview truncation.

## Behavior

No intended behavior change. Agent event shape, tool detail shape, redaction behavior,
path and command summary behavior, and ACP lifecycle are unchanged.
