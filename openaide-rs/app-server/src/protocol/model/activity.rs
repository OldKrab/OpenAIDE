use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Running,
    Completed,
    Interrupted,
    Error,
}

/// User-readable events observed for one delegated Agent thread.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentActivity {
    Delegated,
    Interacted,
    Running,
    Completed,
    Failed,
    Stopped,
}

/// Optional semantic chrome for a Tool row. It never changes Tool identity or detail routing.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ToolPresentation {
    /// Ordered, proven semantic actions performed by one underlying Tool call.
    pub actions: Vec<ToolPresentationAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolPresentationAction {
    Skill {
        subjects: Vec<String>,
    },
    Read {
        subjects: Vec<String>,
    },
    View {
        subjects: Vec<String>,
    },
    List {
        subjects: Vec<String>,
    },
    Search {
        query: String,
        scopes: Vec<String>,
        target: ToolSearchTarget,
    },
}

impl ToolPresentation {
    pub(crate) fn single(kind: ToolPresentationKind, subjects: Vec<String>) -> Self {
        let action = match kind {
            ToolPresentationKind::Skill => ToolPresentationAction::Skill { subjects },
            ToolPresentationKind::Read => ToolPresentationAction::Read { subjects },
            ToolPresentationKind::View => ToolPresentationAction::View { subjects },
            ToolPresentationKind::List => ToolPresentationAction::List { subjects },
            ToolPresentationKind::Search => {
                unreachable!("search presentations require structured query and scope facts")
            }
        };
        Self {
            actions: vec![action],
        }
    }
}

impl ToolPresentationAction {
    pub(crate) fn kind(&self) -> ToolPresentationKind {
        match self {
            Self::Skill { .. } => ToolPresentationKind::Skill,
            Self::Read { .. } => ToolPresentationKind::Read,
            Self::View { .. } => ToolPresentationKind::View,
            Self::List { .. } => ToolPresentationKind::List,
            Self::Search { .. } => ToolPresentationKind::Search,
        }
    }

    pub(crate) fn subjects(&self) -> Option<&[String]> {
        match self {
            Self::Skill { subjects }
            | Self::Read { subjects }
            | Self::View { subjects }
            | Self::List { subjects } => Some(subjects),
            Self::Search { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolSearchTarget {
    Contents,
    Paths,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolPresentationKind {
    Skill,
    Read,
    View,
    List,
    Search,
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
        presentation: Option<ToolPresentation>,
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
    /// One delegated Agent thread. Correlation identity stays on the parent message.
    Subagent {
        /// New protocol-faithful fields are optional so tasks saved by the first
        /// Subagent renderer remain readable after upgrade.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<String>,
        #[serde(default)]
        name: String,
        #[serde(default)]
        path: Vec<String>,
        status: ActivityStatus,
        // The first Subagent release persisted rows before event history existed.
        // Treat those rows as delegation observations so upgrades remain readable.
        #[serde(default = "default_subagent_events")]
        events: Vec<SubagentActivity>,
    },
}

fn default_subagent_events() -> Vec<SubagentActivity> {
    vec![SubagentActivity::Delegated]
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
