use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use crate::agent::acp_schema::{
    InitializeRequest, InitializeResponse, McpServer, SessionConfigOption,
};
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_session_lifecycle::{
    initialize_supports_session_close, initialize_supports_session_delete, load_active_session,
    resume_active_session, start_active_session, validate_initialize_protocol,
    LoadActiveSessionRequest, LoadReplayCaptures,
};
use crate::agent::acp_trace::AcpTraceSession;
use crate::protocol::errors::RuntimeError;

pub(super) type AcpActiveSession = agent_client_protocol::ActiveSession<'static, Agent>;

pub(super) async fn initialize_agent_connection(
    connection: &ConnectionTo<Agent>,
    request: InitializeRequest,
    trace: Option<&AcpTraceSession>,
    start_error_tx: &mpsc::Sender<Result<impl Send, RuntimeError>>,
) -> Result<InitializeResponse, agent_client_protocol::Error> {
    let started_at = Instant::now();
    crate::logging::info(
        "acp_initialize_started",
        serde_json::json!({
            "source": "session_start",
            "task_id": trace.map(AcpTraceSession::task_id),
        }),
    );
    if let Some(trace) = trace {
        trace.record("client_to_agent", "initialize.request", &request);
    }
    let initialize = match connection.send_request(request).block_task().await {
        Ok(initialize) => initialize,
        Err(error) => {
            crate::logging::warn(
                "acp_initialize_failed",
                serde_json::json!({
                    "duration_ms": started_at.elapsed().as_millis(),
                    "error_kind": "protocol_error",
                    "task_id": trace.map(AcpTraceSession::task_id),
                }),
            );
            let _ = start_error_tx.send(Err(crate::agent::acp_errors::acp_error(&error)));
            return Err(error);
        }
    };
    if let Some(trace) = trace {
        trace.record("agent_to_client", "initialize.response", &initialize);
    }
    if let Err(error) = validate_initialize_protocol(&initialize) {
        crate::logging::warn(
            "acp_initialize_failed",
            serde_json::json!({
                "duration_ms": started_at.elapsed().as_millis(),
                "error_kind": "invalid_capabilities",
                "task_id": trace.map(AcpTraceSession::task_id),
            }),
        );
        let _ = start_error_tx.send(Err(error.clone()));
        return Err(agent_client_protocol::util::internal_error(
            error.to_string(),
        ));
    }
    crate::logging::info(
        "acp_initialize_completed",
        serde_json::json!({
            "duration_ms": started_at.elapsed().as_millis(),
            "auth_method_count": initialize.auth_methods.len(),
            "task_id": trace.map(AcpTraceSession::task_id),
        }),
    );
    Ok(initialize)
}

pub(super) struct AcpSessionRunner<'a> {
    agent_id: &'a str,
    connection: &'a ConnectionTo<Agent>,
    initialize: InitializeResponse,
    auth_method_id: Option<&'a str>,
    trace: Option<&'a AcpTraceSession>,
}

impl<'a> AcpSessionRunner<'a> {
    pub(super) fn new(
        agent_id: &'a str,
        connection: &'a ConnectionTo<Agent>,
        initialize: InitializeResponse,
        auth_method_id: Option<&'a str>,
        trace: Option<&'a AcpTraceSession>,
    ) -> Self {
        Self {
            agent_id,
            connection,
            initialize,
            auth_method_id,
            trace,
        }
    }

    pub(super) fn initialize(&self) -> &InitializeResponse {
        &self.initialize
    }

    pub(super) fn supports_session_close(&self) -> bool {
        initialize_supports_session_close(&self.initialize)
    }

    pub(super) fn supports_session_delete(&self) -> bool {
        initialize_supports_session_delete(&self.initialize)
    }

    pub(super) async fn start(
        &self,
        cwd: PathBuf,
        mcp_servers: Vec<McpServer>,
    ) -> Result<(AcpActiveSession, Vec<SessionConfigOption>), agent_client_protocol::Error> {
        start_active_session(
            self.connection,
            cwd,
            &self.initialize,
            self.auth_method_id,
            mcp_servers,
            self.trace,
        )
        .await
    }

    pub(super) async fn load(
        &self,
        session_id: String,
        cwd: PathBuf,
        mcp_servers: Vec<McpServer>,
        load_replay: &LoadReplayCaptures,
    ) -> Result<
        (
            AcpActiveSession,
            crate::protocol::model::ConfigOptionsCatalog,
            Option<crate::protocol::model::AgentCommandsCatalog>,
            crate::agent::acp_update_projection::ReplayProjectionResult,
        ),
        RuntimeError,
    > {
        load_active_session(
            self.connection,
            &self.initialize,
            load_replay,
            self.trace,
            LoadActiveSessionRequest {
                agent_id: self.agent_id,
                session_id,
                cwd,
                mcp_servers,
                preferred_auth_method_id: self.auth_method_id,
            },
        )
        .await
    }

    pub(super) async fn resume(
        &self,
        session_id: String,
        cwd: PathBuf,
        mcp_servers: Vec<McpServer>,
    ) -> Result<
        (
            AcpActiveSession,
            Option<crate::protocol::model::ConfigOptionsCatalog>,
        ),
        RuntimeError,
    > {
        resume_active_session(
            self.connection,
            &self.initialize,
            self.trace,
            super::acp_session_lifecycle::ResumeActiveSessionRequest {
                agent_id: self.agent_id,
                session_id,
                cwd,
                mcp_servers,
                preferred_auth_method_id: self.auth_method_id,
            },
        )
        .await
    }
}

pub(super) fn acp_start_error(error: RuntimeError) -> agent_client_protocol::Error {
    agent_client_protocol::util::internal_error(error.to_string())
}
