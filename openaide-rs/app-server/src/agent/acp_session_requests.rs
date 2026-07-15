use std::path::PathBuf;

use crate::agent::acp_schema::{
    AuthenticateRequest, ErrorCode, InitializeResponse, ListSessionsRequest, ListSessionsResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
    ResumeSessionRequest, ResumeSessionResponse, SessionId,
};
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_session_capabilities::auth_method_for_session_retry;
use crate::agent::acp_trace::AcpTraceSession;

pub(super) async fn request_new_session(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<NewSessionResponse, agent_client_protocol::Error> {
    match send_new_session_request(connection, cwd.clone(), trace).await {
        Ok(response) => Ok(response),
        Err(error) if error.code == ErrorCode::AuthRequired => {
            authenticate_for_retry(connection, initialize, preferred_auth_method_id, error).await?;
            send_new_session_request(connection, cwd, trace).await
        }
        Err(error) => Err(error),
    }
}

pub(super) async fn request_load_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
    match send_load_session_request(connection, session_id.clone(), cwd.clone(), trace).await {
        Ok(response) => Ok(response),
        Err(error) if error.code == ErrorCode::AuthRequired => {
            authenticate_for_retry(connection, initialize, preferred_auth_method_id, error).await?;
            send_load_session_request(connection, session_id, cwd, trace).await
        }
        Err(error) => Err(error),
    }
}

pub(super) async fn request_resume_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    trace: Option<&AcpTraceSession>,
) -> Result<ResumeSessionResponse, agent_client_protocol::Error> {
    match send_resume_session_request(connection, session_id.clone(), cwd.clone(), trace).await {
        Ok(response) => Ok(response),
        Err(error) if error.code == ErrorCode::AuthRequired => {
            authenticate_for_retry(connection, initialize, preferred_auth_method_id, error).await?;
            send_resume_session_request(connection, session_id, cwd, trace).await
        }
        Err(error) => Err(error),
    }
}

pub(super) async fn request_session_list(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    cursor: Option<String>,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
) -> Result<ListSessionsResponse, agent_client_protocol::Error> {
    match send_session_list_request(connection, cwd.clone(), cursor.clone()).await {
        Ok(response) => Ok(response),
        Err(error) if error.code == ErrorCode::AuthRequired => {
            authenticate_for_retry(connection, initialize, preferred_auth_method_id, error).await?;
            send_session_list_request(connection, cwd, cursor).await
        }
        Err(error) => Err(error),
    }
}

async fn authenticate_for_retry(
    connection: &ConnectionTo<Agent>,
    initialize: &InitializeResponse,
    preferred_auth_method_id: Option<&str>,
    auth_error: agent_client_protocol::Error,
) -> Result<(), agent_client_protocol::Error> {
    let Some(method_id) = auth_method_for_session_retry(initialize, preferred_auth_method_id)
    else {
        return Err(auth_error);
    };
    connection
        .send_request(AuthenticateRequest::new(method_id))
        .block_task()
        .await?;
    Ok(())
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
