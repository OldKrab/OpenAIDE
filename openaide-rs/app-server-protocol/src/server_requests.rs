use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::ClientInstanceId;

pub const PERMISSION_REQUEST: &str = "permission/request";
pub const QUESTION_REQUEST: &str = "question/request";
pub const SECRET_READ: &str = "secret/read";
pub const SHELL_SHOW_NOTIFICATION: &str = "shell/showNotification";
pub const SHELL_REVEAL_FILE: &str = "shell/revealFile";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestParams {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    pub tool_call: PermissionToolCallRef,
    pub options: Vec<PermissionRequestOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PermissionToolCallRef {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestOption {
    pub option_id: String,
    pub name: String,
    pub kind: PermissionRequestOptionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PermissionRequestOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestResponse {
    pub option_id: String,
}

/// A normalized form question. App shells never receive the unstable ACP schema directly.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequestParams {
    pub message: String,
    pub fields: Vec<QuestionField>,
}

// JSON cannot carry NaN, so all floating-point values admitted at this protocol seam are Eq.
impl Eq for QuestionRequestParams {}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum QuestionField {
    String {
        key: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        required: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_length: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<QuestionStringFormat>,
    },
    Number {
        key: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        required: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<f64>,
    },
    Integer {
        key: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        required: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<i64>,
    },
    Boolean {
        key: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        required: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    SingleSelect {
        key: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        required: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        options: Vec<QuestionOption>,
    },
    MultiSelect {
        key: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        required: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        default: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_items: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_items: Option<u64>,
        options: Vec<QuestionOption>,
    },
}

impl Eq for QuestionField {}

impl QuestionField {
    pub fn key(&self) -> &str {
        match self {
            Self::String { key, .. }
            | Self::Number { key, .. }
            | Self::Integer { key, .. }
            | Self::Boolean { key, .. }
            | Self::SingleSelect { key, .. }
            | Self::MultiSelect { key, .. } => key,
        }
    }

    pub fn required(&self) -> bool {
        match self {
            Self::String { required, .. }
            | Self::Number { required, .. }
            | Self::Integer { required, .. }
            | Self::Boolean { required, .. }
            | Self::SingleSelect { required, .. }
            | Self::MultiSelect { required, .. } => *required,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum QuestionStringFormat {
    Email,
    Uri,
    Date,
    DateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum QuestionRequestResponse {
    Submit {
        content: BTreeMap<String, QuestionValue>,
    },
    Cancel,
}

impl Eq for QuestionRequestResponse {}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(untagged)]
pub enum QuestionValue {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    StringArray(Vec<String>),
}

impl Eq for QuestionValue {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecretReadParams {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecretReadResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellShowNotificationParams {
    pub level: ShellNotificationLevel,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ShellNotificationAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ShellNotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellNotificationAction {
    pub action_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellShowNotificationResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellRevealFileParams {
    pub originating_client_instance_id: ClientInstanceId,
    pub file_handle_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellRevealFileResponse {
    pub revealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellResolveFileRevealParams {
    pub originating_client_instance_id: ClientInstanceId,
    pub file_handle_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellResolveFileRevealResult {
    pub path: String,
    pub label: String,
}

#[cfg(test)]
#[path = "server_requests_tests.rs"]
mod tests;
