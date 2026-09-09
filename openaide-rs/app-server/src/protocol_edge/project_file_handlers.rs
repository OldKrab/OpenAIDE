use super::{responses, RpcGateway};
use crate::{client_lifecycle::ConnectionId, logging};
use openaide_app_server_protocol::{
    envelopes::RequestMeta,
    errors::ProtocolError,
    ids::ClientInstanceId,
    project_files::{ProjectFilesParams, ProjectFilesResult},
};
use serde_json::Value;
use std::{path::PathBuf, time::Instant};

pub(super) struct PreparedProjectFiles {
    pub owner: ClientInstanceId,
    root: PathBuf,
    params: ProjectFilesParams,
    method: String,
}
impl PreparedProjectFiles {
    pub fn run(&self, id: &str, meta: &RequestMeta) -> ProjectFilesResult {
        let started = Instant::now();
        logging::info(
            "project_files_started",
            serde_json::json!({"request_id":id,"client_request_id":meta.client_request_id,"client_id":self.owner,"operation":self.method,"attempt":1}),
        );
        let result = crate::project_files::run(&self.root, &self.method, &self.params);
        logging::info(
            "project_files_completed",
            serde_json::json!({"request_id":id,"client_request_id":meta.client_request_id,"client_id":self.owner,"operation":self.method,"attempt":1,"outcome":if result.error.is_some(){"failure"}else{"success"},"error_kind":result.error,"truncated":result.truncated,"count":result.entries.len(),"duration_ms":started.elapsed().as_millis()}),
        );
        result
    }
}
impl RpcGateway {
    pub(super) fn prepare_project_files(
        &self,
        connection: &ConnectionId,
        method: &str,
        params: Value,
    ) -> Result<PreparedProjectFiles, ProtocolError> {
        let client = self
            .client_hub
            .context_for_connection(connection)
            .ok_or_else(|| responses::not_initialized(method.to_string()))?;
        let params: ProjectFilesParams =
            serde_json::from_value(params).map_err(responses::invalid_params)?;
        let root = self
            .task_snapshots
            .workspace_root_for_client(&client.client_instance_id, &params.task_id)?;
        Ok(PreparedProjectFiles {
            owner: client.client_instance_id,
            root: PathBuf::from(root),
            params,
            method: method.into(),
        })
    }
}
