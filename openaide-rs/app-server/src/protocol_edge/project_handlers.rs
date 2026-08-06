use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::project::{
    ProjectAddParams, ProjectAddResult, ProjectRefreshParams, ProjectRefreshResult,
    ProjectRemoveParams, ProjectRemoveResult, ProjectRenameParams, ProjectRenameResult,
};
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_project_add(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectAddParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let added = match self.project_roots.add_project(&params.workspace_root) {
            Ok(project) => project,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let snapshot = match self.snapshots.project_collection_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let Some(project) = snapshot
            .projects
            .into_iter()
            .find(|project| project.project_id == added.project_id)
        else {
            return self.error(
                connection_id,
                id,
                meta,
                ProtocolError {
                    code: ProtocolErrorCode::Internal,
                    message: "Added Project was missing from the Project snapshot".to_string(),
                    recoverable: true,
                    target: None,
                },
            );
        };
        let events = self
            .publish_project_collection_update(now)
            .unwrap_or_default();
        crate::logging::info(
            "project_added",
            serde_json::json!({
                "project_id": project.project_id.as_str(),
                "workspace_root": project.workspace_root,
            }),
        );
        responses::result_with_events(
            connection_id,
            id,
            meta,
            ProjectAddResult { project },
            events,
        )
    }

    pub(super) fn handle_project_rename(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectRenameParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let project = match self.project_context_from_snapshot(&params.project_id) {
            Ok(project) => project,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        if let Err(error) = self.project_roots.rename_project(&project, &params.label) {
            return self.error(connection_id, id, meta, error);
        }
        let project = match self.project_summary(&params.project_id) {
            Ok(project) => project,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self
            .publish_project_collection_update(now)
            .unwrap_or_default();
        crate::logging::info(
            "project_renamed",
            serde_json::json!({
                "project_id": project.project_id.as_str(),
                "label": project.label,
            }),
        );
        responses::result_with_events(
            connection_id,
            id,
            meta,
            ProjectRenameResult { project },
            events,
        )
    }

    pub(super) fn handle_project_remove(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectRemoveParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        if let Err(error) = self.project_context_from_snapshot(&params.project_id) {
            return self.error(connection_id, id, meta, error);
        }
        let removed_task_count = match self.project_roots.remove_project(&params.project_id) {
            Ok(count) => count,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self
            .publish_project_collection_update(now)
            .unwrap_or_default();
        crate::logging::info(
            "project_removed",
            serde_json::json!({
                "project_id": params.project_id.as_str(),
                "removed_task_count": removed_task_count,
            }),
        );
        responses::result_with_events(
            connection_id,
            id,
            meta,
            ProjectRemoveResult { removed_task_count },
            events,
        )
    }

    pub(super) fn handle_project_refresh(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectRefreshParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let project = match self.project_summary(&params.project_id) {
            Ok(project) => project,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self
            .publish_project_collection_update(now)
            .unwrap_or_default();
        responses::result_with_events(
            connection_id,
            id,
            meta,
            ProjectRefreshResult { project },
            events,
        )
    }

    fn project_context_from_snapshot(
        &self,
        project_id: &openaide_app_server_protocol::ids::ProjectId,
    ) -> Result<crate::projects::ProjectTaskContext, ProtocolError> {
        let summary = self.project_summary(project_id)?;
        Ok(crate::projects::ProjectTaskContext {
            project_id: summary.project_id,
            workspace_root: summary.workspace_root,
            label: summary.label,
            isolation: crate::protocol::model::IsolationKind::Local,
        })
    }

    fn project_summary(
        &self,
        project_id: &openaide_app_server_protocol::ids::ProjectId,
    ) -> Result<openaide_app_server_protocol::snapshot::ProjectSummary, ProtocolError> {
        self.snapshots
            .project_collection_snapshot()?
            .projects
            .into_iter()
            .find(|project| project.project_id == *project_id)
            .ok_or_else(|| ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: format!("Project not found: {}", project_id.as_str()),
                recoverable: false,
                target: None,
            })
    }
}
