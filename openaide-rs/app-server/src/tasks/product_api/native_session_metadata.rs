use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::ProjectId;

use crate::native_sessions::catalog::NativeSessionRef;
use crate::protocol::errors::RuntimeError;
use crate::snapshots::native_session_summary;

use super::{
    protocol_error_from_runtime, set_title::normalize_user_title, NativeSessionMetadataMutation,
    TaskProductApi,
};

impl TaskProductApi {
    pub(super) fn set_native_session_title(
        &self,
        agent_id: &str,
        native_session_id: &str,
        title: String,
    ) -> Result<NativeSessionMetadataMutation, ProtocolError> {
        let title = normalize_user_title(title, "title")?;
        self.set_native_session_metadata(agent_id, native_session_id, Some(title), None)
    }

    pub(super) fn set_native_session_pinned(
        &self,
        agent_id: &str,
        native_session_id: &str,
        pinned: bool,
    ) -> Result<NativeSessionMetadataMutation, ProtocolError> {
        self.set_native_session_metadata(agent_id, native_session_id, None, Some(pinned))
    }

    fn set_native_session_metadata(
        &self,
        agent_id: &str,
        native_session_id: &str,
        title: Option<String>,
        pinned: Option<bool>,
    ) -> Result<NativeSessionMetadataMutation, ProtocolError> {
        let _mutation = self.native_adoption.lock().map_err(|_| {
            protocol_error_from_runtime(RuntimeError::Internal(
                "Native Session mutation lock poisoned".to_string(),
            ))
        })?;
        let reference = NativeSessionRef::new(agent_id, native_session_id);
        let title_changed = title.is_some();
        let pin_changed = pinned.is_some();
        self.native_catalog
            .entry(&reference)
            .ok_or_else(not_found)?;
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
                message: "This Native Session belongs to an OpenAIDE Task".to_string(),
                recoverable: false,
                target: None,
            });
        }
        self.native_catalog
            .set_metadata(&reference, title, pinned)
            .map_err(protocol_error_from_runtime)?;
        let entry = self
            .native_catalog
            .entry(&reference)
            .ok_or_else(not_found)?;
        let project_id = ProjectId::from(entry.project_id.clone());
        let mutation = NativeSessionMetadataMutation {
            session: native_session_summary(entry, project_id.clone()),
            project_id,
        };
        crate::logging::info(
            "native_session_metadata_changed",
            serde_json::json!({
                "agent_id": agent_id,
                "native_session_id": native_session_id,
                "title_changed": title_changed,
                "pin_changed": pin_changed,
            }),
        );
        Ok(mutation)
    }
}

fn not_found() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotFound,
        message: "Native Session was not found in OpenAIDE discovery".to_string(),
        recoverable: false,
        target: None,
    }
}
