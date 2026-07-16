use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::TaskReleaseParams;

use crate::agent::AgentSessionKey;
use crate::time::now_string;

use super::{protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    pub(super) fn release_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskReleaseParams,
    ) -> Result<(), ProtocolError> {
        let task_id = params.task_id.as_str();
        let now = now_string();
        let disposed = self
            .mutations
            .release_prepared_task(client_instance_id, task_id, &now)
            .map_err(protocol_error_from_runtime)?;
        // Legacy pre-Send resources must never cross a lease boundary. The client-owned
        // Image design does not create any new resource here.
        self.attachments.discard_resources_for_task(&params.task_id);
        self.close_disposed_prepared_tasks(disposed);
        Ok(())
    }

    pub(super) fn close_disposed_prepared_tasks(
        &self,
        disposed: Vec<crate::storage::records::TaskRecord>,
    ) {
        for task in disposed {
            let Some(session_id) = task.agent_session_id else {
                continue;
            };
            let session = AgentSessionKey::new(task.agent_id, session_id.clone());
            if let Err(error) = self.agent_gateway.close_session(&session) {
                crate::logging::warn(
                    "prepared_task_native_session_close_failed",
                    serde_json::json!({
                        "task_id": task.task_id,
                        "error": error.to_string(),
                    }),
                );
            }
        }
    }
}
