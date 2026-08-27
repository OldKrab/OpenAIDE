# Draft Subagent Protocol Is Isolated

Status: accepted

Until ACP Subagent Sessions ship in released Rust crates, OpenAIDE pins the exact reviewed commit from agentclientprotocol/agent-client-protocol PR #1992 and isolates its temporary types at the existing ACP schema seam; it never follows a moving branch or maintains independent copies of the draft wire contract. Once the complete App Server and Frontend path is ready, OpenAIDE advertises only canonical `clientCapabilities.subagents: {}` to every ACP Agent and relies on bilateral negotiation rather than a vendor capability alias.

An Agent that violates the negotiated native lifecycle does not trigger a mid-session downgrade or duplicate legacy projection. OpenAIDE rejects only invalid child traffic, emits redacted structured diagnostics, preserves Main Agent work, and resolves an announced child as disconnected when connection loss or authoritative load leaves its outcome unknown; raw protocol errors do not become product copy or leave the Task permanently blocked.
