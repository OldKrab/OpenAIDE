# P41 Next Slice Selection

Completed: 2026-06-27T03:22:22+03:00

## Selected Slice

Split tool input/output detail and sanitization helpers out of
`agent/tool_details.rs`.

## Rationale

`agent/tool_details.rs` is now one of the remaining oversized Agent modules. Unlike
the ACP runtime/session modules, it is a pure mapping module: it converts ACP
`ToolCall` data into OpenAIDE `AgentEvent` and `ActivityToolDetails` shapes. That
makes it a good next low-risk cleanup before touching ACP lifecycle-heavy files.

The clearest boundary inside the file is raw tool input/output summarization and
redaction. It is cohesive, has no runtime state, and is already called through a few
private helpers from the main tool-event mapping function.

## Non-Selection

Do not split ACP runtime/session lifecycle in this slice.

Do not change the tool event shape or redaction behavior in this slice. Any changes to
support-export, diagnostics, or UI tool detail semantics need a separate contract.
