# The Draft Subagent Form Is Removed

Status: accepted; supersedes the draft-form parts of [ADR-0048](0048-draft-subagent-protocol-is-isolated.md) and [ADR-0067](0067-native-subagents-use-released-acp-crates.md)

Both maintained adapters now send OpenAIDE only the released ACP Subagent form, and keep the draft `subagent_spawned` and `subagent_state_update` updates for clients that ask for that form. App Server therefore no longer reads the draft form: a stale custom adapter that sends it gets the same treatment as any other `session/update` the schema cannot decode, one `acp_session_update_undecodable` warning per kind and no child. A second input path would only keep unreachable behavior alive, including the lifecycle Chat row and the Codex notices that substituted for a prompt the draft form could not carry.

Durable history does not change. Saved `Delegated to` lifecycle rows stay in Chat, and a row still running at upgrade closes when its child ends. What stands from ADR-0048: invalid child traffic is rejected with redacted diagnostics and never causes a mid-session downgrade.
