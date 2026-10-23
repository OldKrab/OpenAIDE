use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::snapshot::TaskSummary;

/// Asks for the current Task Attention Events of open Tasks (ADR-0027).
///
/// It serves an App Shell that cannot keep a Frontend running, such as a phone
/// whose WebView is paused while hidden. A request carrying the cursor of the
/// set the client already has is held by App Server until that set changes or
/// a bounded wait ends, so an idle client costs one small exchange per wait.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttentionWaitParams {
    /// The cursor of the last result; absent for the first request, which answers at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttentionWaitResult {
    /// Names this set of events; equal cursors mean an unchanged set.
    pub cursor: String,
    /// Every open Task that has a current Task Attention Event, and only those.
    pub tasks: Vec<TaskSummary>,
}
