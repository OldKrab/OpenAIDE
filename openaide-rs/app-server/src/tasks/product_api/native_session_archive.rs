use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentId, ProjectId};
use openaide_app_server_protocol::snapshot::NativeSessionReference;

use crate::native_sessions::catalog::NativeSessionRef;
use crate::protocol::errors::RuntimeError;

use super::{protocol_error_from_runtime, NativeSessionArchiveMutation, TaskProductApi};

impl TaskProductApi {
    /// Changes only OpenAIDE-owned catalog visibility; this boundary never contacts the Agent.
    pub(super) fn set_native_session_archived(
        &self,
        agent_id: &str,
        native_session_id: &str,
        archived: bool,
    ) -> Result<NativeSessionArchiveMutation, ProtocolError> {
        // Serialize ownership and visibility decisions so adoption cannot race an archive request.
        let _native_session_mutation = self.native_adoption.lock().map_err(|_| {
            protocol_error_from_runtime(RuntimeError::Internal(
                "Native Session mutation lock poisoned".to_string(),
            ))
        })?;
        let reference = NativeSessionRef::new(agent_id, native_session_id);
        let entry = self
            .native_catalog
            .entry(&reference)
            .ok_or_else(|| ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: "Native Session was not found in OpenAIDE discovery".to_string(),
                recoverable: false,
                target: None,
            })?;
        let owned = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .any(|task| {
                !task.tombstoned
                    && task.agent_id == agent_id
                    && task.agent_session_id.as_deref() == Some(native_session_id)
            });
        if owned {
            return Err(ProtocolError {
                code: ProtocolErrorCode::Conflict,
                message:
                    "This Native Session belongs to an OpenAIDE Task and follows Task Archive behavior"
                        .to_string(),
                recoverable: false,
                target: None,
            });
        }
        if archived {
            self.native_catalog
                .archive(&reference)
                .map_err(protocol_error_from_runtime)?;
        } else {
            self.native_catalog
                .restore(&reference)
                .map_err(protocol_error_from_runtime)?;
        }
        crate::logging::info(
            "native_session_archive_changed",
            serde_json::json!({
                "agent_id": agent_id,
                "native_session_id": native_session_id,
                "archived": archived,
            }),
        );
        Ok(NativeSessionArchiveMutation {
            reference: NativeSessionReference {
                agent_id: AgentId::from(agent_id),
                session_id: native_session_id.to_string(),
            },
            project_id: ProjectId::from(entry.project_id),
            archived,
        })
    }
}
