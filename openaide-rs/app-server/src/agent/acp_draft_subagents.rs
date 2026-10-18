//! The draft ACP Subagent form: `subagent_spawned` and `subagent_state_update`.
//!
//! The released ACP schema replaced these updates with `subagent_update`, so
//! typed decoding no longer knows them. The connection reads them here from the
//! raw `session/update`, ahead of typed dispatch and in arrival order with the
//! session's other updates.
// TODO: delete when @openaide/codex-acp sends the released subagents form; until
// then Codex Subagents depend on this second wire form and its raw decoding.

use agent_client_protocol::UntypedMessage;
use serde::Deserialize;

const SPAWNED_KIND: &str = "subagent_spawned";
const STATE_UPDATE_KIND: &str = "subagent_state_update";

/// One draft Subagent update and the session that sent it.
#[derive(Debug)]
pub(super) struct DraftSubagentNotification {
    pub(super) session_id: String,
    pub(super) update: DraftSubagentUpdate,
}

#[derive(Debug)]
pub(super) enum DraftSubagentUpdate {
    Spawned(DraftSubagentSpawned),
    StateChanged(DraftSubagentStateUpdate),
}

impl DraftSubagentNotification {
    /// The child session a `subagent_spawned` announces.
    pub(super) fn announced_child(&self) -> Option<&str> {
        match &self.update {
            DraftSubagentUpdate::Spawned(spawned) => Some(&spawned.subagent_session_id),
            DraftSubagentUpdate::StateChanged(_) => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DraftSubagentSpawned {
    pub(super) subagent_session_id: String,
    pub(super) name: String,
    pub(super) task: String,
    #[serde(default)]
    pub(super) capabilities: DraftSubagentCapabilities,
    /// Adapter extension: the exact prompt the parent delegated. Kept raw so a
    /// malformed value loses the prompt, not the announcement.
    #[serde(default)]
    prompt: Option<serde_json::Value>,
    /// Kept raw: a malformed `_meta` loses its details, not the announcement.
    #[serde(default, rename = "_meta")]
    pub(super) meta: Option<serde_json::Value>,
}

impl DraftSubagentSpawned {
    /// The delegated prompt, when the adapter attached a non-empty one.
    pub(super) fn prompt(&self) -> Option<String> {
        self.prompt
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .filter(|prompt| !prompt.trim().is_empty())
            .map(str::to_string)
    }
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct DraftSubagentCapabilities {
    #[serde(default)]
    pub(super) cancel: bool,
    #[serde(default)]
    pub(super) close: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DraftSubagentStateUpdate {
    pub(super) subagent_session_id: String,
    pub(super) state: DraftSubagentState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DraftSubagentState {
    Completed,
    Failed,
    Cancelled,
    Disconnected,
    /// A state this client does not know. The child can no longer be followed.
    #[serde(other)]
    Unrecognized,
}

/// Reads a draft Subagent update from a raw `session/update`. `None` means the
/// notification is something else; an error means a draft update that cannot
/// be read, which the caller drops.
pub(super) fn decode_draft_subagent_update(
    notification: &UntypedMessage,
) -> Option<Result<DraftSubagentNotification, serde_json::Error>> {
    if notification.method() != "session/update" {
        return None;
    }
    let params = notification.params();
    let update = params.get("update")?;
    let kind = update.get("sessionUpdate")?.as_str()?;
    if kind != SPAWNED_KIND && kind != STATE_UPDATE_KIND {
        return None;
    }
    Some(decode(params, update, kind))
}

fn decode(
    params: &serde_json::Value,
    update: &serde_json::Value,
    kind: &str,
) -> Result<DraftSubagentNotification, serde_json::Error> {
    let session_id =
        String::deserialize(params.get("sessionId").unwrap_or(&serde_json::Value::Null))?;
    let update = if kind == SPAWNED_KIND {
        DraftSubagentUpdate::Spawned(DraftSubagentSpawned::deserialize(update)?)
    } else {
        DraftSubagentUpdate::StateChanged(DraftSubagentStateUpdate::deserialize(update)?)
    };
    Ok(DraftSubagentNotification { session_id, update })
}

#[cfg(test)]
#[path = "acp_draft_subagents_tests.rs"]
mod tests;
