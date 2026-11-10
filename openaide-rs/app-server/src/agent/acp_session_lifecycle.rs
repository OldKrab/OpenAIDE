use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::agent::acp_schema::{
    InitializeResponse, ListSessionsResponse, McpServer, SessionConfigOption, SessionId,
    SessionNotification, SessionUpdate,
};
use agent_client_protocol::v1::{ActiveSession, ConnectionTo, SessionSetup};
use agent_client_protocol::Agent;

use crate::agent::acp_errors::acp_request_error;
use crate::agent::acp_replayed_subagents::project_replayed_subagents;
pub(super) use crate::agent::acp_session_capabilities::{
    auth_method_kind, initialize_supports_session_close, initialize_supports_session_delete,
    validate_initialize_protocol, validate_load_session_capability,
    validate_resume_session_capability,
};
use crate::agent::acp_session_meta::session_request_meta;
pub(super) use crate::agent::acp_session_requests::request_session_list;
use crate::agent::acp_session_requests::{
    request_load_session, request_new_session, request_resume_session,
};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::{
    normalize_available_commands, normalize_config_options, ReplayProjection,
    ReplayProjectionResult,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentCommandsCatalog, AgentListSessionsResult, AgentListedSession, ConfigOptionsCatalog,
};

// TODO: read the root replay from the `ActiveSession` that `load_session_from` returns (the SDK
// routes the session before sending `session/load`) once child-session traffic can be kept out of
// the live Subagent router during a load some other way. The capture must stay for now: the
// connection's notification handler routes Subagent announcements, `session_message` updates and
// every child session's updates to that router, and child updates carry the child's session id,
// so none of them reach the root session's update stream; the replayed delegation rows need them,
// in arrival order, in `subagent_traffic`.
pub(super) struct LoadReplayCapture {
    pub(super) session_id: SessionId,
    pub(super) updates: Vec<SessionUpdate>,
    /// Native child streams are isolated from the main replay projection.
    pub(super) subagent_session_ids: HashSet<String>,
    /// The root's Subagent updates and every child's updates, in arrival order.
    /// The Task restores these children once the complete replay commits.
    pub(super) subagent_traffic: Vec<SessionNotification>,
}

pub(super) type LoadReplayCaptures = Arc<Mutex<HashMap<String, LoadReplayCapture>>>;

pub(super) async fn start_active_session(
    connection: &ConnectionTo<Agent>,
    agent_id: &str,
    cwd: PathBuf,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    mcp_servers: Vec<McpServer>,
    trace: Option<&AcpTraceSession>,
) -> Result<(ActiveSession<'static, Agent>, Vec<SessionConfigOption>), agent_client_protocol::Error>
{
    let SessionSetup {
        session, response, ..
    } = request_new_session(
        connection,
        cwd,
        initialize,
        preferred_auth_method_id,
        mcp_servers,
        session_request_meta(agent_id),
        trace,
    )
    .await?;
    let initial_options = response.config_options.unwrap_or_default();
    Ok((session, initial_options))
}

pub(super) struct LoadActiveSessionRequest<'a> {
    pub(super) agent_id: &'a str,
    pub(super) session_id: String,
    pub(super) cwd: PathBuf,
    pub(super) mcp_servers: Vec<McpServer>,
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
        ActiveSession<'static, Agent>,
        ConfigOptionsCatalog,
        Option<AgentCommandsCatalog>,
        ReplayProjectionResult,
    ),
    RuntimeError,
> {
    let LoadActiveSessionRequest {
        agent_id,
        session_id,
        cwd,
        mcp_servers,
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
                subagent_session_ids: HashSet::new(),
                subagent_traffic: Vec::new(),
            },
        );
    }

    let response = request_load_session(
        connection,
        session_id.clone(),
        cwd,
        initialize,
        preferred_auth_method_id,
        mcp_servers,
        session_request_meta(agent_id),
        trace,
    )
    .await
    .map_err(|error| acp_request_error(&error));
    let (replayed_updates, subagent_traffic) = load_replay
        .lock()
        .expect("ACP load replay capture lock poisoned")
        .remove(&session_id.to_string())
        .map(|capture| (capture.updates, capture.subagent_traffic))
        .unwrap_or_default();
    let SessionSetup {
        session: active_session,
        response,
        ..
    } = response?;
    let response_options = response.config_options.unwrap_or_default();
    let replayed_command_catalog = latest_command_catalog(&replayed_updates);
    let mut replay = ReplayProjection::for_agent(agent_id, session_id.to_string())
        .project_with_plan(replayed_updates);
    replay.subagents = project_replayed_subagents(
        agent_id,
        initialize
            .agent_info
            .as_ref()
            .map(|info| info.name.as_str()),
        &session_id.to_string(),
        subagent_traffic,
    );
    Ok((
        active_session,
        normalize_config_options(agent_id, response_options),
        replayed_command_catalog,
        replay,
    ))
}

pub(super) struct ResumeActiveSessionRequest<'a> {
    pub(super) agent_id: &'a str,
    pub(super) session_id: String,
    pub(super) cwd: PathBuf,
    pub(super) mcp_servers: Vec<McpServer>,
    pub(super) preferred_auth_method_id: Option<&'a str>,
}

pub(super) async fn resume_active_session(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    trace: Option<&AcpTraceSession>,
    request: ResumeActiveSessionRequest<'_>,
) -> Result<(ActiveSession<'static, Agent>, Option<ConfigOptionsCatalog>), RuntimeError> {
    let ResumeActiveSessionRequest {
        agent_id,
        session_id,
        cwd,
        mcp_servers,
        preferred_auth_method_id,
    } = request;
    validate_resume_session_capability(initialize)?;
    let session_id = SessionId::new(session_id);
    let SessionSetup {
        session: active_session,
        response,
        ..
    } = request_resume_session(
        connection,
        session_id.clone(),
        cwd,
        initialize,
        preferred_auth_method_id,
        mcp_servers,
        session_request_meta(agent_id),
        trace,
    )
    .await
    .map_err(|error| acp_request_error(&error))?;
    let config_catalog = response
        .config_options
        .map(|options| normalize_config_options(agent_id, options));
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
    requested_cwd: Option<&Path>,
    excluded_session_id: Option<&str>,
) -> AgentListSessionsResult {
    // Our managed adapter explicitly marks index-backed pages as observational.
    // Ordinary ACP listings retain the existing complete-scan reconciliation contract.
    let authoritative = response
        .meta
        .as_ref()
        .and_then(|meta| meta.get("io.openaide/session-list"))
        .and_then(|meta| meta.get("authoritative"))
        .and_then(serde_json::Value::as_bool)
        != Some(false);
    let sessions = response
        .sessions
        .into_iter()
        .filter(|session| requested_cwd.is_none_or(|cwd| session.cwd == cwd))
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
        authoritative,
        next_cursor: response.next_cursor,
    }
}
