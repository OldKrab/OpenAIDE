# Subagent History Is Scoped From Task Chat

Status: accepted

Native Subagent messages, reasoning, Tools, and Plans remain in that Subagent's durable history rather than being duplicated into Task Chat. Task Chat retains one durable clickable lifecycle row per Subagent at its spawn position and updates that row with the last proven status; each active or completed Plan is likewise scoped to the Main Agent or originating Subagent. Legacy Agents that do not negotiate native Subagent sessions retain their existing Tool-call projection.

A Subagent failure does not determine Task failure because the Main Agent may recover, retry, or treat that work as optional. Task status remains owned by the Main Agent's turn outcome, while Subagent failure remains visible in its navigator entry and Task Chat lifecycle row; a child permission request or question may still produce the ordinary Task waiting state.
