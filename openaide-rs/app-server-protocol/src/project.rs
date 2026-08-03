use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::ProjectId;
use crate::snapshot::ProjectSummary;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRegisterParams {
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRenameParams {
    pub project_id: ProjectId,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReconnectParams {
    pub project_id: ProjectId,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRemoveParams {
    pub project_id: ProjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMutationResult {
    pub project: ProjectSummary,
}
