use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Running,
    Completed,
    Interrupted,
    Error,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityStep {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        level: Option<String>,
    },
    Tool {
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        name: String,
        status: ActivityStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_summary: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail_artifact_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Box<ActivityToolDetails>>,
        permission_outcomes: Vec<ToolPermissionOutcome>,
    },
    Command {
        command_label: String,
        status: ActivityStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_preview: Option<String>,
    },
}

/// Durable authorization decisions associated with one ACP tool call.
/// Execution status remains owned by ACP and is intentionally independent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ToolPermissionOutcome {
    pub request_id: String,
    pub decision: ToolPermissionDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_label: Option<String>,
    pub resolved_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermissionDecision {
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityToolDetails {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<ActivityToolLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<ActivityToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<ActivityToolInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<ActivityToolOutput>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityToolLocation {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityToolContent {
    Text {
        text: String,
    },
    Diff {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        old_text: Option<String>,
        new_text: String,
    },
    Terminal {
        terminal_id: String,
    },
    Image {
        media_type: String,
        data: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
    Audio {
        media_type: String,
        data: String,
    },
    Resource {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        size_bytes: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Unsupported {
        content_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        uri: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityToolInput {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ActivityToolField>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityToolOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatted_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregated_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ActivityToolField>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityToolField {
    pub name: String,
    pub value: ActivityToolValue,
}

/// Safe, typed projection of arbitrary ACP raw tool input and output.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityToolValue {
    Null,
    Boolean { value: bool },
    Number { value: String },
    String { value: String },
    Array { items: Vec<ActivityToolValue> },
    Object { fields: Vec<ActivityToolField> },
    Redacted,
}
