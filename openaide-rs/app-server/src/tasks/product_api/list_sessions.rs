use openaide_app_server_protocol::agent::{
    AgentListSessionsParams, AgentListSessionsResult, AgentListedSession,
};
use openaide_app_server_protocol::errors::ProtocolError;

use crate::agent::{AgentListSessionsRequest, AgentSessionKey};

use super::session_cursor::OpaqueSessionCursor;
use super::{protocol_error_from_runtime, AgentListSessionsWorkflow, TaskProductApi};

impl TaskProductApi {
    pub(super) fn refresh_native_session_catalogs(&self) -> Result<(), ProtocolError> {
        let contexts = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .filter(|task| !task.tombstoned && task.agent_session_id.is_some())
            .map(|task| (task.agent_id, task.workspace_root))
            .collect::<std::collections::HashSet<_>>();
        let mut first_error = None;
        for (agent_id, workspace_root) in contexts {
            match self.fetch_complete_native_catalog(&agent_id, &workspace_root) {
                Ok(sessions) => {
                    self.history_sync
                        .replace_listed_sessions(&agent_id, &workspace_root, sessions)
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn fetch_complete_native_catalog(
        &self,
        agent_id: &str,
        workspace_root: &str,
    ) -> Result<Vec<crate::protocol::model::AgentListedSession>, ProtocolError> {
        let mut cursor = OpaqueSessionCursor::new(None);
        let mut sessions = Vec::new();
        loop {
            let result = self
                .agent_gateway
                .list_sessions(AgentListSessionsRequest {
                    agent_id: agent_id.to_string(),
                    cwd: workspace_root.to_string(),
                    cursor: cursor.current(),
                })
                .map_err(protocol_error_from_runtime)?;
            sessions.extend(result.sessions);
            if cursor.advance(result.next_cursor).is_none() {
                return Ok(sessions);
            }
        }
    }

    fn list_sessions_for_project(
        &self,
        params: AgentListSessionsParams,
    ) -> Result<AgentListSessionsResult, ProtocolError> {
        let project = self
            .project_resolver
            .resolve_task_context(&params.project_id)?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        let agent_id = params.agent_id.clone();
        let mut cursor = OpaqueSessionCursor::new(params.cursor);
        loop {
            let result = self
                .agent_gateway
                .list_sessions(AgentListSessionsRequest {
                    agent_id: params.agent_id.as_str().to_string(),
                    cwd: project.workspace_root.clone(),
                    cursor: cursor.current(),
                })
                .map_err(protocol_error_from_runtime)?;
            let next_cursor = cursor.advance(result.next_cursor);
            self.history_sync.record_listed_sessions(
                params.agent_id.as_str(),
                &project.workspace_root,
                &result.sessions,
            );
            let sessions = self
                .unowned_native_sessions(params.agent_id.as_str(), result.sessions)?
                .into_iter()
                .map(|session| AgentListedSession {
                    session_id: session.session_id,
                    title: session.title,
                    last_activity: session.last_activity,
                    updated_at: session.updated_at,
                })
                .collect::<Vec<_>>();
            if !sessions.is_empty() || next_cursor.is_none() {
                return Ok(AgentListSessionsResult {
                    agent_id,
                    project_id: project.project_id,
                    project_label: project.label,
                    sessions,
                    next_cursor,
                });
            }
        }
    }

    fn unowned_native_sessions(
        &self,
        agent_id: &str,
        sessions: Vec<crate::protocol::model::AgentListedSession>,
    ) -> Result<Vec<crate::protocol::model::AgentListedSession>, ProtocolError> {
        let records = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?;
        let mut owned: std::collections::HashSet<AgentSessionKey> = records
            .into_iter()
            .filter(|record| record.agent_id == agent_id)
            .filter_map(|record| {
                record
                    .agent_session_id
                    .map(|session_id| AgentSessionKey::new(record.agent_id, session_id))
            })
            .collect();
        owned.extend(
            self.preparing_session_ids
                .lock()
                .map_err(|_| {
                    protocol_error_from_runtime(crate::protocol::errors::RuntimeError::Internal(
                        "preparing session ownership lock poisoned".to_string(),
                    ))
                })?
                .iter()
                .cloned(),
        );
        Ok(sessions
            .into_iter()
            .filter(|session| {
                !owned.contains(&AgentSessionKey::new(agent_id, session.session_id.clone()))
            })
            .collect())
    }
}

impl AgentListSessionsWorkflow for TaskProductApi {
    fn list_agent_sessions(
        &self,
        params: AgentListSessionsParams,
    ) -> Result<AgentListSessionsResult, ProtocolError> {
        self.list_sessions_for_project(params)
    }

    fn refresh_native_session_catalogs(&self) -> Result<(), ProtocolError> {
        TaskProductApi::refresh_native_session_catalogs(self)
    }
}
