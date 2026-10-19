# Native Subagents Use Released ACP Crates

Status: accepted; supersedes the dependency pin in the first paragraph of [ADR-0048](0048-draft-subagent-protocol-is-isolated.md)

ACP released Subagent Sessions in schema 1.10 in a form that replaced the draft OpenAIDE pinned: `subagent_update` patches a child's title, description, capabilities, and running or idle state, and `session_message` carries a message one session sends another. OpenAIDE now builds against released ACP crates and no longer pins the PR #1992 commit. The released schema has no draft types, so the draft `subagent_spawned` and `subagent_state_update` updates are read from the raw `session/update` in one module at the Agent integration seam, ahead of typed dispatch and in arrival order with the session's other updates. That module exists only while the Codex adapter still sends the draft and is deleted when the adapter moves to the released form.

The released form drops the Agent-side `sessionCapabilities.subagents`, so negotiation is the client capability alone: while the internal rollback override is unset, OpenAIDE advertises `clientCapabilities.subagents: {}`, and an Agent that supports either form sends child sessions once offered. The rest of ADR-0048 stands: invalid child traffic is rejected with redacted diagnostics and never downgrades the session.
