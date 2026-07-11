use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ClientInstanceId, RequestId, TaskId};
use crate::server_requests::{PermissionRequestParams, QuestionRequestParams};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PendingRequestSnapshot {
    pub request_id: RequestId,
    pub scope: PendingRequestScope,
    pub kind: PendingRequestKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionRequestParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<QuestionRequestParams>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PendingRequestScope {
    Client {
        client_instance_id: ClientInstanceId,
    },
    Task {
        task_id: TaskId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PendingRequestKind {
    Permission,
    Question,
    Secret,
    ShellCapability,
}
