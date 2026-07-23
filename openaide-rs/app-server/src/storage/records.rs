use openaide_app_server_protocol::ids::ClientInstanceId;
use serde::{Deserialize, Serialize};

use crate::protocol::model::{
    AgentCommandsCatalog, ChatMessage, ConfigOptionCurrentValue, ConfigOptionsCatalog,
    IsolationKind, TaskStatus, TaskSummary,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status")]
#[derive(Default)]
pub enum TaskPreparationRecord {
    Needed,
    Preparing,
    #[default]
    Ready,
    Blocked {
        reason: TaskPreparationBlockerRecord,
        message: String,
    },
    Failed {
        message: String,
        /// The empty Prepared Task can be replaced because its Agent-owned session vanished.
        #[serde(default)]
        native_session_missing: bool,
    },
}

/// Stable task-level preparation failures that can be resolved by the user.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPreparationBlockerRecord {
    AuthRequired,
    SetupRequired,
    NodeJsRequired,
}

/// Controls whether a Task belongs to normal product history or the Prepared-Task pool.
///
/// A New Task is visible only to its optional lessee until the first user message is accepted.
/// `owner_client_instance_id` is accepted as a migration alias for records written before #17;
/// process startup clears every persisted lease before serving clients.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TaskLifecycle {
    #[serde(alias = "new")]
    Prepared {
        #[serde(
            default,
            alias = "owner_client_instance_id",
            skip_serializing_if = "Option::is_none"
        )]
        lease: Option<ClientInstanceId>,
    },
    #[serde(alias = "visible")]
    Open,
    Archived,
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
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open)
    }

    pub fn is_archived(&self) -> bool {
        matches!(self, Self::Archived)
    }

    pub fn is_listed(&self) -> bool {
        matches!(self, Self::Open | Self::Archived)
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
    #[serde(deserialize_with = "deserialize_pending_config_value")]
    pub requested_value: ConfigOptionCurrentValue,
}

fn deserialize_pending_config_value<'de, D>(
    deserializer: D,
) -> Result<ConfigOptionCurrentValue, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if let Some(value) = value.as_str() {
        return Ok(ConfigOptionCurrentValue::id(value));
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

/// The latest product-level reason a Task needs user attention.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskAttentionReason {
    Finished,
    NeedsPermission,
    NeedsAnswer,
    Stopped,
    Failed,
}

/// Durable identity lets App Shells deduplicate an attention event across reconnects.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskAttentionEvent {
    pub event_id: String,
    pub reason: TaskAttentionReason,
    pub occurred_at: String,
}

impl TaskAttentionEvent {
    pub fn new(
        event_id: impl Into<String>,
        reason: TaskAttentionReason,
        occurred_at: impl Into<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            reason,
            occurred_at: occurred_at.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRecord {
    pub task_id: String,
    pub title: Option<TaskTitle>,
    pub status: TaskStatus,
    pub task_version: u64,
    pub message_history_version: u64,
    pub unread: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<TaskAttentionEvent>,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity: String,
    pub agent_id: String,
    pub agent_name: String,
    pub isolation: IsolationKind,
    pub workspace_root: String,
    /// Stable Project identity root; differs from `workspace_root` for Worktree Tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_root: Option<String>,
    /// Durable Task Workspace identity. Legacy Project-root Tasks omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    pub lifecycle: TaskLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_id: Option<String>,
    /// Durable wall-clock origin for active-turn elapsed-time presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_started_at: Option<String>,
    #[serde(default)]
    pub tombstoned: bool,
    #[serde(default)]
    pub revision: u64,
    /// Last catalog from the bound live Native Session. Process recovery clears
    /// it before any snapshot can use it; it is never input to session creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_options_catalog: Option<ConfigOptionsCatalog>,
    #[serde(default, skip_serializing_if = "TaskConfigMutationState::is_empty")]
    pub config_mutation: TaskConfigMutationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_commands_catalog: Option<AgentCommandsCatalog>,
    pub model_id: Option<String>,
    /// Native Session prompt capabilities captured during preparation.
    #[serde(default)]
    pub supports_image_input: bool,
    #[serde(default, skip_serializing_if = "is_default_preparation")]
    pub preparation: TaskPreparationRecord,
}

impl<'de> Deserialize<'de> for TaskRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Read-only compatibility shape for records written before Archive became lifecycle.
        #[derive(Deserialize)]
        struct StoredTaskRecord {
            task_id: String,
            title: Option<TaskTitle>,
            status: TaskStatus,
            task_version: u64,
            message_history_version: u64,
            unread: bool,
            #[serde(default)]
            attention: Option<TaskAttentionEvent>,
            created_at: String,
            updated_at: String,
            last_activity: String,
            agent_id: String,
            agent_name: String,
            isolation: IsolationKind,
            workspace_root: String,
            #[serde(default)]
            project_root: Option<String>,
            #[serde(default)]
            worktree_id: Option<String>,
            lifecycle: TaskLifecycle,
            #[serde(default)]
            agent_session_id: Option<String>,
            #[serde(default)]
            active_turn_id: Option<String>,
            #[serde(default)]
            active_turn_started_at: Option<String>,
            #[serde(default)]
            archived: bool,
            #[serde(default)]
            tombstoned: bool,
            #[serde(default)]
            revision: u64,
            #[serde(default)]
            config_options_catalog: Option<ConfigOptionsCatalog>,
            #[serde(default)]
            config_mutation: TaskConfigMutationState,
            #[serde(default)]
            agent_commands_catalog: Option<AgentCommandsCatalog>,
            model_id: Option<String>,
            #[serde(default)]
            supports_image_input: bool,
            #[serde(default)]
            preparation: TaskPreparationRecord,
        }

        let stored = StoredTaskRecord::deserialize(deserializer)?;
        let lifecycle = if stored.archived && matches!(stored.lifecycle, TaskLifecycle::Open) {
            TaskLifecycle::Archived
        } else {
            stored.lifecycle
        };
        Ok(Self {
            task_id: stored.task_id,
            title: stored.title,
            status: stored.status,
            task_version: stored.task_version,
            message_history_version: stored.message_history_version,
            unread: stored.unread,
            attention: stored.attention,
            created_at: stored.created_at,
            updated_at: stored.updated_at,
            last_activity: stored.last_activity,
            agent_id: stored.agent_id,
            agent_name: stored.agent_name,
            isolation: stored.isolation,
            workspace_root: stored.workspace_root,
            project_root: stored.project_root,
            worktree_id: stored.worktree_id,
            lifecycle,
            agent_session_id: stored.agent_session_id,
            active_turn_id: stored.active_turn_id,
            active_turn_started_at: stored.active_turn_started_at,
            tombstoned: stored.tombstoned,
            revision: stored.revision,
            config_options_catalog: stored.config_options_catalog,
            config_mutation: stored.config_mutation,
            agent_commands_catalog: stored.agent_commands_catalog,
            model_id: stored.model_id,
            supports_image_input: stored.supports_image_input,
            preparation: stored.preparation,
        })
    }
}

impl TaskRecord {
    /// Starts a new App Server epoch without carrying live Agent controls across processes.
    ///
    /// These fields describe an attached Native Session, not durable Chat. Clearing them in
    /// the lightweight catalog keeps startup independent from Task-history size; the same
    /// overlay is applied if that Task is hydrated later in this process.
    pub(crate) fn clear_process_local_agent_state(&mut self) -> bool {
        let had_config_catalog = self.config_options_catalog.take().is_some();
        let had_commands_catalog = self.agent_commands_catalog.take().is_some();
        let had_pending_mutation = self.config_mutation.pending.take().is_some();
        had_config_catalog || had_commands_catalog || had_pending_mutation
    }

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
            attention: self.attention.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_activity: self.last_activity.clone(),
            agent_id: self.agent_id.clone(),
            agent_name: self.agent_name.clone(),
            isolation: self.isolation,
            workspace_root: self.workspace_root.clone(),
            project_root: self.project_root.clone(),
            worktree_id: self.worktree_id.clone(),
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
