use std::sync::Arc;

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentId, ClientInstanceId, ProjectId};
use openaide_app_server_protocol::snapshot::NativeSessionReference;
use openaide_app_server_protocol::task::{NativeSessionForkParams, NativeSessionForkSource};

use crate::agent::{AgentSecretResolver, AgentSessionFork};
use crate::native_sessions::catalog::NativeSessionRef;
use crate::projects::ProjectIdentity;
use crate::protocol::errors::RuntimeError;

use super::{protocol_error_from_runtime, NativeSessionForkMutation, TaskProductApi};

struct ResolvedForkSource {
    agent_id: String,
    session_id: String,
    project_id: ProjectId,
    workspace_root: String,
    source_title: String,
    secret_resolver: Arc<dyn AgentSecretResolver>,
}

impl TaskProductApi {
    pub(super) fn fork_native_session(
        &self,
        client_instance_id: &ClientInstanceId,
        params: NativeSessionForkParams,
    ) -> Result<NativeSessionForkMutation, ProtocolError> {
        // One lock keeps archive, adoption, and fork source ownership decisions consistent.
        let _native_session_mutation = self.native_adoption.lock().map_err(|_| {
            protocol_error_from_runtime(RuntimeError::Internal(
                "Native Session mutation lock poisoned".to_string(),
            ))
        })?;
        let source = self.resolve_fork_source(client_instance_id, params.source)?;
        let started_at = std::time::Instant::now();
        crate::logging::info(
            "native_session_fork_started",
            serde_json::json!({
                "agent_id": source.agent_id,
                "source_session_id": source.session_id,
                "project_id": source.project_id.as_str(),
            }),
        );
        let forked = self
            .agent_gateway
            .fork_session(AgentSessionFork {
                agent_id: source.agent_id.clone(),
                source_session_id: source.session_id.clone(),
                cwd: source.workspace_root.clone(),
                secret_resolver: Some(source.secret_resolver),
            })
            .map_err(protocol_error_from_runtime)?;
        let reference = NativeSessionRef::new(&source.agent_id, &forked.session_id);
        self.native_catalog
            .record_fork(
                source.project_id.as_str(),
                &source.workspace_root,
                reference,
                format!("Fork of {}", source.source_title),
            )
            .map_err(protocol_error_from_runtime)?;
        self.task_notifier
            .navigation_project_entries_changed(source.project_id.as_str().to_string());
        crate::logging::info(
            "native_session_fork_completed",
            serde_json::json!({
                "agent_id": source.agent_id,
                "source_session_id": source.session_id,
                "forked_session_id": forked.session_id,
                "project_id": source.project_id.as_str(),
                "close_warning": forked.close_warning,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        Ok(NativeSessionForkMutation {
            reference: NativeSessionReference {
                agent_id: AgentId::from(source.agent_id),
                session_id: forked.session_id,
            },
            project_id: source.project_id,
            close_warning: forked.close_warning,
        })
    }

    fn resolve_fork_source(
        &self,
        client_instance_id: &ClientInstanceId,
        source: NativeSessionForkSource,
    ) -> Result<ResolvedForkSource, ProtocolError> {
        match source {
            NativeSessionForkSource::Task { task_id } => {
                let task =
                    self.read_interactive_task_for_client(task_id.as_str(), client_instance_id)?;
                let session_id = task.agent_session_id.clone().ok_or_else(|| ProtocolError {
                    code: ProtocolErrorCode::Conflict,
                    message: "This Task does not have a Native Session to fork".to_string(),
                    recoverable: false,
                    target: None,
                })?;
                let project_root = task.project_root.as_deref().unwrap_or(&task.workspace_root);
                Ok(ResolvedForkSource {
                    agent_id: task.agent_id,
                    session_id,
                    project_id: ProjectIdentity::from_workspace_root(project_root).project_id,
                    workspace_root: task.workspace_root,
                    source_title: task
                        .title
                        .effective()
                        .map(|title| title.value().to_string())
                        .unwrap_or_else(|| "Untitled task".to_string()),
                    secret_resolver: super::secret_resolver::task_secret_resolver(
                        &self.server_requests,
                        &self.store,
                        task_id.as_str(),
                    ),
                })
            }
            NativeSessionForkSource::NativeSession {
                agent_id,
                native_session_id,
            } => {
                let reference = NativeSessionRef::new(agent_id.as_str(), &native_session_id);
                let entry = self
                    .native_catalog
                    .entry(&reference)
                    .ok_or_else(|| ProtocolError {
                        code: ProtocolErrorCode::NotFound,
                        message: "Native Session was not found in OpenAIDE discovery".to_string(),
                        recoverable: false,
                        target: None,
                    })?;
                let owned_by_task = self
                    .store
                    .list_all_task_records()
                    .map_err(protocol_error_from_runtime)?
                    .into_iter()
                    .any(|task| {
                        task.agent_id == agent_id.as_str()
                            && task.agent_session_id.as_deref() == Some(native_session_id.as_str())
                    });
                if owned_by_task {
                    return Err(ProtocolError {
                        code: ProtocolErrorCode::Conflict,
                        message: "Fork this session from its OpenAIDE Task".to_string(),
                        recoverable: false,
                        target: None,
                    });
                }
                if self.native_catalog.is_archived(&reference) {
                    return Err(ProtocolError {
                        code: ProtocolErrorCode::Conflict,
                        message: "Restore this Native Session before forking it".to_string(),
                        recoverable: false,
                        target: None,
                    });
                }
                let project_id = ProjectId::from(entry.project_id.clone());
                let source_title = entry
                    .observation
                    .title
                    .clone()
                    .or(entry.local_fallback_title)
                    .unwrap_or_else(|| "Untitled task".to_string());
                Ok(ResolvedForkSource {
                    agent_id: agent_id.as_str().to_string(),
                    session_id: native_session_id,
                    project_id: project_id.clone(),
                    workspace_root: entry.workspace_root,
                    source_title,
                    secret_resolver: self.task_secret_resolver_for_project(
                        &format!("native-session-fork-{}", uuid::Uuid::new_v4()),
                        project_id,
                    ),
                })
            }
        }
    }
}
