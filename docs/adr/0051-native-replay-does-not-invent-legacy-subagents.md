# Native Replay Does Not Invent Legacy Subagents

Status: accepted

Passive Task open preserves existing flattened Codex Subagent Tool rows and never fabricates native Subagent identities or histories from incomplete legacy metadata. New negotiated turns may add native Subagents alongside that older evidence. An explicit authoritative `session/load` may replace the saved projection with replayed native histories only after the complete replay validates and commits, preserving existing history on failure and never retaining both native and legacy representations of the same replayed lifecycle. The replacement adds the children the Task does not hold through the same path as their live traffic, and leaves a child it holds as saved, because replacing it would need one commit across every Subagent stream.
