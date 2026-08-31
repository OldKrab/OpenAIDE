use std::path::PathBuf;
use std::time::Instant;

use crate::agent::acp_schema::{
    InitializeResponse, ListSessionsRequest, ListSessionsResponse, LoadSessionRequest,
    LoadSessionResponse, McpServer, NewSessionRequest, NewSessionResponse, ResumeSessionRequest,
    ResumeSessionResponse, SessionId,
};
use agent_client_protocol::{Agent, ConnectionTo};

use crate::agent::acp_trace::AcpTraceSession;
use crate::logging;

pub(super) async fn request_new_session(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
    mcp_servers: Vec<McpServer>,
    trace: Option<&AcpTraceSession>,
) -> Result<NewSessionResponse, agent_client_protocol::Error> {
    send_new_session_request(connection, cwd, mcp_servers, trace).await
}

pub(super) async fn request_load_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
    mcp_servers: Vec<McpServer>,
    trace: Option<&AcpTraceSession>,
) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
    send_load_session_request(connection, session_id, cwd, mcp_servers, trace).await
}

pub(super) async fn request_resume_session(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
    mcp_servers: Vec<McpServer>,
    trace: Option<&AcpTraceSession>,
) -> Result<ResumeSessionResponse, agent_client_protocol::Error> {
    send_resume_session_request(connection, session_id, cwd, mcp_servers, trace).await
}

pub(super) async fn request_session_list(
    connection: &ConnectionTo<Agent>,
    cwd: Option<PathBuf>,
    cursor: Option<String>,
    _initialize: &InitializeResponse,
    _preferred_auth_method_id: Option<&str>,
) -> Result<ListSessionsResponse, agent_client_protocol::Error> {
    send_session_list_request(connection, cwd, cursor).await
}

async fn send_new_session_request(
    connection: &ConnectionTo<Agent>,
    cwd: PathBuf,
    mcp_servers: Vec<McpServer>,
    trace: Option<&AcpTraceSession>,
) -> Result<NewSessionResponse, agent_client_protocol::Error> {
    let started_at = Instant::now();
    logging::info(
        "acp_session_request_started",
        serde_json::json!({
            "operation": "session/new",
            "task_id": trace.map(AcpTraceSession::task_id),
            "mcp_server_count": mcp_servers.len(),
        }),
    );
    let request = NewSessionRequest::new(cwd).mcp_servers(mcp_servers);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/new.request", &request);
    }
    let response = match connection.send_request(request).block_task().await {
        Ok(response) => response,
        Err(error) => {
            logging::warn(
                "acp_session_request_failed",
                serde_json::json!({
                    "operation": "session/new",
                    "task_id": trace.map(AcpTraceSession::task_id),
                    "duration_ms": started_at.elapsed().as_millis(),
                    "error_kind": "protocol_error",
                }),
            );
            return Err(error);
        }
    };
    if let Some(trace) = trace {
        trace.record("agent_to_client", "session/new.response", &response);
    }
    logging::info(
        "acp_session_request_completed",
        serde_json::json!({
            "operation": "session/new",
            "task_id": trace.map(AcpTraceSession::task_id),
            "duration_ms": started_at.elapsed().as_millis(),
            "config_option_count": response
                .config_options
                .as_ref()
                .map(Vec::len)
                .unwrap_or(0),
        }),
    );
    Ok(response)
}

async fn send_load_session_request(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    mcp_servers: Vec<McpServer>,
    trace: Option<&AcpTraceSession>,
) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
    let started_at = Instant::now();
    logging::info(
        "acp_session_request_started",
        serde_json::json!({
            "operation": "session/load",
            "task_id": trace.map(AcpTraceSession::task_id),
            "mcp_server_count": mcp_servers.len(),
        }),
    );
    let request = LoadSessionRequest::new(session_id, cwd).mcp_servers(mcp_servers);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/load.request", &request);
    }
    let response = match connection.send_request(request).block_task().await {
        Ok(response) => response,
        Err(error) => {
            logging::warn(
                "acp_session_request_failed",
                serde_json::json!({
                    "operation": "session/load",
                    "task_id": trace.map(AcpTraceSession::task_id),
                    "duration_ms": started_at.elapsed().as_millis(),
                    "error_kind": "protocol_error",
                }),
            );
            return Err(error);
        }
    };
    if let Some(trace) = trace {
        trace.record("agent_to_client", "session/load.response", &response);
    }
    logging::info(
        "acp_session_request_completed",
        serde_json::json!({
            "operation": "session/load",
            "task_id": trace.map(AcpTraceSession::task_id),
            "duration_ms": started_at.elapsed().as_millis(),
            "config_option_count": response
                .config_options
                .as_ref()
                .map(Vec::len)
                .unwrap_or(0),
        }),
    );
    Ok(response)
}

async fn send_resume_session_request(
    connection: &ConnectionTo<Agent>,
    session_id: SessionId,
    cwd: PathBuf,
    mcp_servers: Vec<McpServer>,
    trace: Option<&AcpTraceSession>,
) -> Result<ResumeSessionResponse, agent_client_protocol::Error> {
    let started_at = Instant::now();
    logging::info(
        "acp_session_request_started",
        serde_json::json!({
            "operation": "session/resume",
            "task_id": trace.map(AcpTraceSession::task_id),
            "mcp_server_count": mcp_servers.len(),
        }),
    );
    let request = ResumeSessionRequest::new(session_id, cwd).mcp_servers(mcp_servers);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/resume.request", &request);
    }
    let response = match connection.send_request(request).block_task().await {
        Ok(response) => response,
        Err(error) => {
            logging::warn(
                "acp_session_request_failed",
                serde_json::json!({
                    "operation": "session/resume",
                    "task_id": trace.map(AcpTraceSession::task_id),
                    "duration_ms": started_at.elapsed().as_millis(),
                    "error_kind": "protocol_error",
                }),
            );
            return Err(error);
        }
    };
    if let Some(trace) = trace {
        trace.record("agent_to_client", "session/resume.response", &response);
    }
    logging::info(
        "acp_session_request_completed",
        serde_json::json!({
            "operation": "session/resume",
            "task_id": trace.map(AcpTraceSession::task_id),
            "duration_ms": started_at.elapsed().as_millis(),
            "config_option_count": response
                .config_options
                .as_ref()
                .map(Vec::len)
                .unwrap_or(0),
        }),
    );
    Ok(response)
}

async fn send_session_list_request(
    connection: &ConnectionTo<Agent>,
    cwd: Option<PathBuf>,
    cursor: Option<String>,
) -> Result<ListSessionsResponse, agent_client_protocol::Error> {
    let request = ListSessionsRequest::new().cursor(cursor);
    let request = match cwd {
        Some(cwd) => request.cwd(cwd),
        None => request,
    };
    connection.send_request(request).block_task().await
}
