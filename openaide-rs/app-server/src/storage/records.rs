use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use openaide_app_server_protocol::ids::ClientInstanceId;

use crate::protocol::model::{
    AgentCommandsCatalog, ChatMessage, ConfigOptionsCatalog, IsolationKind, TaskStatus, TaskSummary,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status")]
#[derive(Default)]
pub enum TaskPreparationRecord {
    Needed,
    Preparing,
    #[default]
    Ready,
    Failed {
        message: String,
    },
}

/// Controls whether a Task belongs to normal product history or to one client's New Task surface.
///
/// New Tasks already have durable App Server and Native Session identities, but they stay private
/// until the first user message is accepted. The owner is persistence-only authorization data and
/// is never exposed in client snapshots.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TaskLifecycle {
    New {
        owner_client_instance_id: ClientInstanceId,
    },
    Visible,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskTitleSource {
    Prompt,
    Agent,
    User,
}

/// A durable Task title with its owner. Construction enforces the stored title invariant.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TaskTitle {
    value: String,
    source: TaskTitleSource,
}

impl TaskTitle {
    pub fn new(value: impl Into<String>, source: TaskTitleSource) -> Option<Self> {
        let value = value.into().trim().to_string();
        (!value.is_empty()).then_some(Self { value, source })
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn source(&self) -> TaskTitleSource {
        self.source
    }
}

impl<'de> Deserialize<'de> for TaskTitle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SerializedTaskTitle {
            value: String,
            source: TaskTitleSource,
        }

        let title = SerializedTaskTitle::deserialize(deserializer)?;
        Self::new(title.value, title.source)
            .ok_or_else(|| serde::de::Error::custom("Task title must be non-empty"))
    }
}

impl TaskLifecycle {
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Visible)
    }
}

/// App Server ordering state for one Task's Agent-owned configuration changes.
///
/// The sequence is monotonic across settled changes so a late Agent response can
/// never become authoritative again after a newer client mutation supersedes it.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskConfigMutationState {
    #[serde(default)]
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingTaskConfigChange>,
}

impl TaskConfigMutationState {
    fn is_empty(&self) -> bool {
        self.sequence == 0 && self.pending.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PendingTaskConfigChange {
    pub sequence: u64,
    pub client_mutation_id: String,
    pub config_id: String,
    pub requested_value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskRecord {
    pub task_id: String,
    pub title: Option<TaskTitle>,
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
    pub lifecycle: TaskLifecycle,
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
    #[serde(default, skip_serializing_if = "TaskConfigMutationState::is_empty")]
    pub config_mutation: TaskConfigMutationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_commands_catalog: Option<AgentCommandsCatalog>,
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_preparation")]
    pub preparation: TaskPreparationRecord,
}

impl TaskRecord {
    /// Applies an Agent title over provisional or Agent-owned titles, preserving user ownership.
    pub fn set_agent_title(&mut self, value: &str) -> bool {
        if self
            .title
            .as_ref()
            .is_some_and(|title| title.source() == TaskTitleSource::User)
        {
            return false;
        }
        let Some(next) = TaskTitle::new(value, TaskTitleSource::Agent) else {
            return false;
        };
        if self.title.as_ref() == Some(&next) {
            return false;
        }
        self.title = Some(next);
        true
    }

    /// Applies an Agent clear over provisional or Agent-owned titles, preserving user ownership.
    pub fn clear_agent_title(&mut self) -> bool {
        if self
            .title
            .as_ref()
            .is_none_or(|title| title.source() == TaskTitleSource::User)
        {
            return false;
        }
        self.title = None;
        true
    }

    pub fn summary(&self) -> TaskSummary {
        TaskSummary {
            task_id: self.task_id.clone(),
            title: self.title.clone(),
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
    pub local_history_updated_at: String,
    pub first_cursor: Option<String>,
    pub last_cursor: Option<String>,
}
