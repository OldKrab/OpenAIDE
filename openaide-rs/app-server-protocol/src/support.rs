use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::snapshot::TaskSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportRecoverStuckSessionsParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SupportRecoverStuckSessionsResult {
    pub recovered_tasks: Vec<TaskSnapshot>,
}
