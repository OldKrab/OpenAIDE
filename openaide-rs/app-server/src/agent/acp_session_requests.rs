use std::path::PathBuf;

use crate::agent::acp_schema::{
    InitializeResponse, ListSessionsRequest, ListSessionsResponse, LoadSessionRequest,
    LoadSessionResponse, NewSessionRequest, NewSessionResponse, ResumeSessionRequest,
    ResumeSessionResponse, SessionId,
};
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_trace::AcpTraceSession;

pub(super) async fn request_new_session(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<NewSessionResponse, agent_client_protocol::Error> {
    send_new_session_request(connection, cwd, trace).await
}

pub(super) async fn request_load_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
    send_load_session_request(connection, session_id, cwd, trace).await
}

pub(super) async fn request_resume_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<ResumeSessionResponse, agent_client_protocol::Error> {
    send_resume_session_request(connection, session_id, cwd, trace).await
}

pub(super) async fn request_session_list(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    cursor: Option<String>,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
) -> Result<ListSessionsResponse, agent_client_protocol::Error> {
    send_session_list_request(connection, cwd, cursor).await
}

async fn send_new_session_request(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    trace: Option<&AcpTraceSession>,
) -> Result<NewSessionResponse, agent_client_protocol::Error> {
    let request = NewSessionRequest::new(cwd);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/new.request", &request);
    }
    let response = connection.send_request(request).block_task().await?;
    if let Some(trace) = trace {
        trace.record("agent_to_client", "session/new.response", &response);
    }
    Ok(response)
}

async fn send_load_session_request(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    trace: Option<&AcpTraceSession>,
) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
    let request = LoadSessionRequest::new(session_id, cwd);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/load.request", &request);
    }
    let response = connection.send_request(request).block_task().await?;
    if let Some(trace) = trace {
        trace.record("agent_to_client", "session/load.response", &response);
    }
    Ok(response)
}

async fn send_resume_session_request(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    trace: Option<&AcpTraceSession>,
) -> Result<ResumeSessionResponse, agent_client_protocol::Error> {
    let request = ResumeSessionRequest::new(session_id, cwd);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/resume.request", &request);
    }
    let response = connection.send_request(request).block_task().await?;
    if let Some(trace) = trace {
        trace.record("agent_to_client", "session/resume.response", &response);
    }
    Ok(response)
}

async fn send_session_list_request(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    cursor: Option<String>,
) -> Result<ListSessionsResponse, agent_client_protocol::Error> {
    connection
        .send_request(ListSessionsRequest::new().cwd(cwd).cursor(cursor))
        .block_task()
        .await
}
