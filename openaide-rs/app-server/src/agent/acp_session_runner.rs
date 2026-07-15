use std::path::PathBuf;
use std::sync::mpsc;

use crate::agent::acp_schema::{
    InitializeRequest, InitializeResponse, SessionConfigOption, SessionId,
};
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_session_lifecycle::{
    initialize_supports_session_close, initialize_supports_session_delete, load_active_session,
    resume_active_session, start_active_session, validate_initialize_protocol,
    LoadActiveSessionRequest, LoadReplayCaptures,
};
use crate::agent::acp_session_termination::close_active_session;
use crate::agent::acp_trace::AcpTraceSession;
use crate::protocol::errors::RuntimeError;

pub(super) type AcpActiveSession = agent_client_protocol::ActiveSession<'static, Agent>;

pub(super) async fn initialize_agent_connection(
    connection: &ConnectionTo<Agent>,
    request: InitializeRequest,
    trace: Option<&AcpTraceSession>,
    start_error_tx: &mpsc::Sender<Result<impl Send, RuntimeError>>,
) -> Result<InitializeResponse, agent_client_protocol::Error> {
    if let Some(trace) = trace {
        trace.record("client_to_agent", "initialize.request", &request);
    }
    let initialize = match connection.send_request(request).block_task().await {
        Ok(initialize) => initialize,
        Err(error) => {
            let _ = start_error_tx.send(Err(crate::agent::acp_errors::acp_error(&error)));
            return Err(error);
        }
    };
    if let Some(trace) = trace {
        trace.record("agent_to_client", "initialize.response", &initialize);
    }
    if let Err(error) = validate_initialize_protocol(&initialize) {
        let _ = start_error_tx.send(Err(error.clone()));
        return Err(agent_client_protocol::util::internal_error(
            error.to_string(),
        ));
    }
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
    ) -> Result<(AcpActiveSession, Vec<SessionConfigOption>), agent_client_protocol::Error> {
        start_active_session(
            self.connection,
            cwd,
            &self.initialize,
            self.auth_method_id,
            self.trace,
        )
        .await
    }

    pub(super) async fn load(
        &self,
        session_id: String,
        cwd: PathBuf,
        load_replay: &LoadReplayCaptures,
    ) -> Result<
        (
            AcpActiveSession,
            crate::protocol::model::ConfigOptionsCatalog,
            Option<crate::protocol::model::AgentCommandsCatalog>,
            Vec<crate::protocol::model::NormalizedMessage>,
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
                preferred_auth_method_id: self.auth_method_id,
            },
        )
        .await
    }

    pub(super) async fn resume(
        &self,
        session_id: String,
        cwd: PathBuf,
    ) -> Result<
        (
            AcpActiveSession,
            Option<crate::protocol::model::ConfigOptionsCatalog>,
        ),
        RuntimeError,
    > {
        resume_active_session(
            self.agent_id,
            self.connection,
            &self.initialize,
            session_id,
            cwd,
            self.auth_method_id,
            self.trace,
        )
        .await
    }

    pub(super) async fn close(&self, session_id: SessionId) {
        close_active_session(
            self.connection,
            session_id,
            self.supports_session_close(),
            self.trace,
        )
        .await;
    }
}

pub(super) fn acp_start_error(error: RuntimeError) -> agent_client_protocol::Error {
    agent_client_protocol::util::internal_error(error.to_string())
}
