//! Task Attention Events for App Shells that cannot keep a Frontend running (ADR-0027).
//!
//! The gateway answers `taskAttention/wait` at once with the current set. Holding a
//! request whose cursor is still current belongs to the transport that can afford a
//! blocked request (see `local_http/attention_wait.rs`); every other transport simply
//! gets the immediate answer, which stays correct for a client that repeats the call.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::task::{
    TaskAttentionWaitParams, TaskAttentionWaitResult, TaskListLifecycle,
};
use serde_json::{json, Value};

use crate::client_lifecycle::ConnectionId;
use crate::logging;
use crate::snapshots::TaskSnapshotSource;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_task_attention_wait(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskAttentionWaitParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match current_task_attention(self.task_snapshots.as_ref()) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        // An unchanged answer is the healthy idle case and stays quiet.
        if params.cursor.as_deref() != Some(result.cursor.as_str()) {
            logging::info(
                "task_attention_wait_changed",
                json!({
                    "connection_id": connection_id.as_str(),
                    "first_request": params.cursor.is_none(),
                    "task_count": result.tasks.len(),
                }),
            );
        }
        self.result::<TaskAttentionWaitResult>(connection_id, id, meta, result)
    }
}

/// Reads the open Tasks that have a current Task Attention Event, and names the set.
///
/// The cursor hashes only Task and event identity, so a Task's title or status
/// changing alone never ends a held wait. It is stable for one App Server build;
/// a client that sends a cursor from another build just gets an immediate answer.
pub(super) fn current_task_attention(
    snapshots: &dyn TaskSnapshotSource,
) -> Result<TaskAttentionWaitResult, ProtocolError> {
    let mut tasks: Vec<_> = snapshots
        .list(TaskListLifecycle::Open, None, None)?
        .tasks
        .into_iter()
        .filter(|task| task.attention.is_some())
        .collect();
    tasks.sort_by(|left, right| left.task_id.as_str().cmp(right.task_id.as_str()));
    let mut hasher = DefaultHasher::new();
    for task in &tasks {
        task.task_id.as_str().hash(&mut hasher);
        if let Some(attention) = &task.attention {
            attention.event_id.hash(&mut hasher);
        }
    }
    Ok(TaskAttentionWaitResult {
        cursor: format!("{:016x}", hasher.finish()),
        tasks,
    })
}

#[cfg(test)]
#[path = "task_attention_handlers_tests.rs"]
pub(super) mod tests;
