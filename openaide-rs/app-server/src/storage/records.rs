use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::protocol::model::{
    AgentCommandsCatalog, ChatMessage, ConfigOptionsCatalog, IsolationKind, TaskStatus, TaskSummary,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum TaskPreparationRecord {
    Needed,
    Preparing,
    Ready,
    Failed { message: String },
}

impl Default for TaskPreparationRecord {
    fn default() -> Self {
        Self::Ready
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskRecord {
    pub task_id: String,
    /// Local title fallback derived from OpenAIDE-owned input or imported history.
    pub title: String,
    /// Agent-owned title takes display priority and may be cleared by session metadata updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_title: Option<String>,
    pub status: TaskStatus,
    pub task_version: u64,
    pub message_history_version: u64,
    pub unread: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity: String,
    pub agent_id: String,
    pub agent_name: String,
    pub isolation: IsolationKind,
    pub workspace_root: String,
    pub first_prompt_sent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_id: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub tombstoned: bool,
    #[serde(default)]
    pub revision: u64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config_options: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_options_catalog: Option<ConfigOptionsCatalog>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_commands_catalog: Option<AgentCommandsCatalog>,
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_preparation")]
    pub preparation: TaskPreparationRecord,
}

impl TaskRecord {
    pub fn effective_title(&self) -> &str {
        self.agent_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .or_else(|| {
                let fallback = self.title.trim();
                (!fallback.is_empty()).then_some(fallback)
            })
            .unwrap_or("Untitled task")
    }

    pub fn summary(&self) -> TaskSummary {
        TaskSummary {
            task_id: self.task_id.clone(),
            title: self.effective_title().to_string(),
            status: self.status,
            task_version: self.task_version,
            message_history_version: self.message_history_version,
            unread: self.unread,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_activity: self.last_activity.clone(),
            agent_id: self.agent_id.clone(),
            agent_name: self.agent_name.clone(),
            isolation: self.isolation,
            workspace_root: self.workspace_root.clone(),
        }
    }
}

fn is_default_preparation(preparation: &TaskPreparationRecord) -> bool {
    matches!(preparation, TaskPreparationRecord::Ready)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StoredMessage {
    pub sequence: u64,
    pub chat: ChatMessage,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MessageMeta {
    pub task_id: String,
    pub version: u64,
    pub message_count: u64,
    pub first_cursor: Option<String>,
    pub last_cursor: Option<String>,
}
