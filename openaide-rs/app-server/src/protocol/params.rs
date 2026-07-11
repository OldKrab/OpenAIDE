use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{Attachment, IsolationKind, PermissionDecision};

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskCreateParams {
    pub mode: TaskCreateMode,
    pub title: String,
    pub workspace_root: String,
    pub selected_agent_id: String,
    #[serde(default)]
    pub selected_agent_label: Option<String>,
    pub selected_isolation: IsolationKind,
    #[serde(default)]
    pub prompt_text: Option<String>,
    #[serde(default)]
    pub external_session_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub config_options: Option<Value>,
    #[serde(default)]
    pub context: Vec<Attachment>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskCreateMode {
    PromptStart,
    AdoptExternalSession,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskIdParams {
    pub task_id: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct TaskListParams {
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskSnapshotParams {
    pub task_id: String,
    #[serde(default = "default_tail_limit")]
    pub tail_limit: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatTailParams {
    pub task_id: String,
    pub limit: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatPageParams {
    pub task_id: String,
    pub before_cursor: String,
    pub limit: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToolDetailParams {
    pub task_id: String,
    pub artifact_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionPromptParams {
    pub task_id: String,
    pub text: String,
    #[serde(default)]
    pub prompt_attachments: Vec<Attachment>,
    #[serde(default)]
    pub message_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskDeleteParams {
    pub task_id: String,
    pub mode: DeleteMode,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PermissionRespondParams {
    pub task_id: String,
    pub request_id: String,
    pub decision: PermissionDecision,
    pub option_id: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RuntimeUpdateSettingsParams {
    #[serde(default)]
    pub developer: RuntimeDeveloperSettingsPatch,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RuntimeDeveloperSettingsPatch {
    #[serde(default)]
    pub acp_trace: RuntimeAcpTraceSettingsPatch,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RuntimeAcpTraceSettingsPatch {
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentConfigOptionsParams {
    pub agent_id: String,
    pub workspace_root: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentProbeParams {
    pub agent_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentAuthenticateParams {
    pub agent_id: String,
    pub method_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentListSessionsParams {
    pub agent_id: String,
    pub workspace_root: String,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionSetConfigOptionParams {
    pub agent_id: String,
    pub workspace_root: String,
    pub config_id: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    Archive,
    Restore,
    Delete,
}

fn default_tail_limit() -> usize {
    100
}
