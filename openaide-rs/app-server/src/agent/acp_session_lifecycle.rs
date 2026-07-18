use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::agent::acp_schema::{
    InitializeResponse, ListSessionsResponse, NewSessionResponse, SessionConfigOption, SessionId,
    SessionUpdate,
};
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_errors::acp_request_error;
pub(super) use crate::agent::acp_session_capabilities::{
    auth_method_kind, initialize_supports_session_close, initialize_supports_session_delete,
    validate_initialize_protocol, validate_load_session_capability,
    validate_resume_session_capability,
};
pub(super) use crate::agent::acp_session_requests::request_session_list;
use crate::agent::acp_session_requests::{
    request_load_session, request_new_session, request_resume_session,
};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::{
    normalize_available_commands, normalize_config_options, ReplayProjection,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentCommandsCatalog, AgentListSessionsResult, AgentListedSession, ConfigOptionsCatalog,
};

pub(super) struct LoadReplayCapture {
    pub(super) session_id: SessionId,
    pub(super) updates: Vec<SessionUpdate>,
}

pub(super) type LoadReplayCaptures = Arc<Mutex<HashMap<String, LoadReplayCapture>>>;

pub(super) async fn start_active_session(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<
    (
        agent_client_protocol::ActiveSession<'static, Agent>,
        Vec<SessionConfigOption>,
    ),
    agent_client_protocol::Error,
> {
    let response =
        request_new_session(connection, cwd, initialize, preferred_auth_method_id, trace).await?;
    let initial_options = response.config_options.clone().unwrap_or_default();
    let active_session = connection.attach_session(response, Vec::new())?;
    Ok((active_session, initial_options))
}

pub(super) struct LoadActiveSessionRequest<'a> {
    pub(super) agent_id: &'a str,
    pub(super) session_id: String,
    pub(super) cwd: PathBuf,
    pub(super) preferred_auth_method_id: Option<&'a str>,
}

pub(super) async fn load_active_session(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    load_replay: &LoadReplayCaptures,
    trace: Option<&AcpTraceSession>,
    request: LoadActiveSessionRequest<'_>,
) -> Result<
    (
        agent_client_protocol::ActiveSession<'static, Agent>,
        ConfigOptionsCatalog,
        Option<AgentCommandsCatalog>,
        Vec<crate::protocol::model::NormalizedMessage>,
    ),
    RuntimeError,
> {
    let LoadActiveSessionRequest {
        agent_id,
        session_id,
        cwd,
        preferred_auth_method_id,
    } = request;
    validate_load_session_capability(initialize)?;
    let session_id = SessionId::new(session_id);
    {
        let mut active = load_replay
            .lock()
            .expect("ACP load replay capture lock poisoned");
        active.insert(
            session_id.to_string(),
            LoadReplayCapture {
                session_id: session_id.clone(),
                updates: Vec::new(),
            },
        );
    }

    let response = request_load_session(
        connection,
        session_id.clone(),
        cwd,
        initialize,
        preferred_auth_method_id,
        trace,
    )
    .await
    .map_err(|error| acp_request_error(&error));
    let replayed_updates = load_replay
        .lock()
        .expect("ACP load replay capture lock poisoned")
        .remove(&session_id.to_string())
        .map(|capture| capture.updates)
        .unwrap_or_default();
    let response = response?;
    let response_options = response.config_options.clone().unwrap_or_default();
    let active_response = NewSessionResponse::new(session_id.clone())
        .modes(response.modes)
        .config_options(response.config_options)
        .meta(response.meta);
    let active_session = connection
        .attach_session(active_response, Vec::new())
        .map_err(|error| acp_request_error(&error))?;
    let replayed_command_catalog = latest_command_catalog(&replayed_updates);
    let replayed_messages = ReplayProjection::new(session_id.to_string()).project(replayed_updates);
    Ok((
        active_session,
        normalize_config_options(agent_id, response_options),
        replayed_command_catalog,
        replayed_messages,
    ))
}

pub(super) async fn resume_active_session(
    agent_id: &str,
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    session_id: String,
    cwd: PathBuf,
    preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<
    (
        agent_client_protocol::ActiveSession<'static, Agent>,
        Option<ConfigOptionsCatalog>,
    ),
    RuntimeError,
> {
    validate_resume_session_capability(initialize)?;
    let session_id = SessionId::new(session_id);
    let response = request_resume_session(
        connection,
        session_id.clone(),
        cwd,
        initialize,
        preferred_auth_method_id,
        trace,
    )
    .await
    .map_err(|error| acp_request_error(&error))?;
    let config_catalog = response
        .config_options
        .clone()
        .map(|options| normalize_config_options(agent_id, options));
    let active_response = NewSessionResponse::new(session_id)
        .modes(response.modes)
        .config_options(response.config_options)
        .meta(response.meta);
    let active_session = connection
        .attach_session(active_response, Vec::new())
        .map_err(|error| acp_request_error(&error))?;
    Ok((active_session, config_catalog))
}

fn latest_command_catalog(updates: &[SessionUpdate]) -> Option<AgentCommandsCatalog> {
    updates.iter().filter_map(command_catalog).next_back()
}

fn command_catalog(update: &SessionUpdate) -> Option<AgentCommandsCatalog> {
    match update {
        SessionUpdate::AvailableCommandsUpdate(update) => {
            Some(normalize_available_commands(update.clone()))
        }
        _ => None,
    }
}

pub(super) fn agent_list_sessions_result_from_response(
    agent_id: String,
    response: ListSessionsResponse,
    requested_cwd: &Path,
    excluded_session_id: Option<&str>,
) -> AgentListSessionsResult {
    let sessions = response
        .sessions
        .into_iter()
        .filter(|session| session.cwd == requested_cwd)
        .filter(|session| {
            excluded_session_id
                .map(|excluded| session.session_id.to_string() != excluded)
                .unwrap_or(true)
        })
        .map(|session| AgentListedSession {
            session_id: session.session_id.to_string(),
            cwd: session.cwd.to_string_lossy().to_string(),
            title: session.title,
            last_activity: session.updated_at.clone(),
            updated_at: session.updated_at,
        })
        .collect::<Vec<_>>();

    AgentListSessionsResult {
        agent_id,
        sessions,
        next_cursor: response.next_cursor,
    }
}
