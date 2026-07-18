use std::time::Duration;

use crate::agent::acp_schema::{
    AuthCapabilities, ClientCapabilities, CreateTerminalRequest, CreateTerminalResponse,
    ElicitationCapabilities, ElicitationFormCapabilities, FileSystemCapabilities,
    InitializeRequest, KillTerminalRequest, KillTerminalResponse, ProtocolVersion,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};

use crate::agent::acp_trace::AcpTraceSession;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

#[cfg(test)]
#[path = "acp_host_tests.rs"]
mod tests;

const HOST_CAPABILITY_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn initialize_request(host_bridge: &HostBridge) -> InitializeRequest {
    let capabilities = ClientCapabilities::new()
        .elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()));
    if !host_bridge.is_enabled() {
        return InitializeRequest::new(ProtocolVersion::V1).client_capabilities(capabilities);
    }

    InitializeRequest::new(ProtocolVersion::V1).client_capabilities(
        capabilities
            .auth(AuthCapabilities::new().terminal(true))
            .fs(FileSystemCapabilities::new()
                .read_text_file(true)
                .write_text_file(true))
            .terminal(true),
    )
}

pub(crate) async fn read_text_file_from_host(
    host_bridge: HostBridge,
    request: ReadTextFileRequest,
    trace: Option<AcpTraceSession>,
) -> Result<ReadTextFileResponse, RuntimeError> {
    host_request(
        host_bridge,
        "fs/read_text_file",
        request,
        Some(HOST_CAPABILITY_TIMEOUT),
        || false,
        trace,
    )
    .await
}

pub(crate) async fn write_text_file_from_host(
    host_bridge: HostBridge,
    request: WriteTextFileRequest,
    trace: Option<AcpTraceSession>,
) -> Result<WriteTextFileResponse, RuntimeError> {
    host_request(
        host_bridge,
        "fs/write_text_file",
        request,
        Some(HOST_CAPABILITY_TIMEOUT),
        || false,
        trace,
    )
    .await
}

pub(crate) async fn create_terminal_from_host(
    host_bridge: HostBridge,
    request: CreateTerminalRequest,
    trace: Option<AcpTraceSession>,
) -> Result<CreateTerminalResponse, RuntimeError> {
    host_request(
        host_bridge,
        "terminal/create",
        request,
        Some(HOST_CAPABILITY_TIMEOUT),
        || false,
        trace,
    )
    .await
}

pub(crate) async fn terminal_output_from_host(
    host_bridge: HostBridge,
    request: TerminalOutputRequest,
    trace: Option<AcpTraceSession>,
) -> Result<TerminalOutputResponse, RuntimeError> {
    host_request(
        host_bridge,
        "terminal/output",
        request,
        Some(HOST_CAPABILITY_TIMEOUT),
        || false,
        trace,
    )
    .await
}

pub(crate) async fn wait_for_terminal_exit_from_host(
    host_bridge: HostBridge,
    request: WaitForTerminalExitRequest,
    is_cancelled: impl Fn() -> bool + Send + 'static,
    trace: Option<AcpTraceSession>,
) -> Result<WaitForTerminalExitResponse, RuntimeError> {
    host_request(
        host_bridge,
        "terminal/wait_for_exit",
        request,
        None,
        is_cancelled,
        trace,
    )
    .await
}

pub(crate) async fn kill_terminal_from_host(
    host_bridge: HostBridge,
    request: KillTerminalRequest,
    trace: Option<AcpTraceSession>,
) -> Result<KillTerminalResponse, RuntimeError> {
    host_request(
        host_bridge,
        "terminal/kill",
        request,
        Some(HOST_CAPABILITY_TIMEOUT),
        || false,
        trace,
    )
    .await
}

pub(crate) async fn release_terminal_from_host(
    host_bridge: HostBridge,
    request: ReleaseTerminalRequest,
    trace: Option<AcpTraceSession>,
) -> Result<ReleaseTerminalResponse, RuntimeError> {
    host_request(
        host_bridge,
        "terminal/release",
        request,
        Some(HOST_CAPABILITY_TIMEOUT),
        || false,
        trace,
    )
    .await
}

pub(crate) async fn host_request<Request, Response>(
    host_bridge: HostBridge,
    method: &'static str,
    request: Request,
    timeout: Option<Duration>,
    is_cancelled: impl Fn() -> bool + Send + 'static,
    trace: Option<AcpTraceSession>,
) -> Result<Response, RuntimeError>
where
    Request: serde::Serialize + Send + 'static,
    Response: serde::de::DeserializeOwned + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let params = serde_json::to_value(request)
            .map_err(|error| RuntimeError::Internal(error.to_string()))?;
        if let Some(trace) = &trace {
            trace.record_value(
                "agent_to_client",
                &format!("{method}.request"),
                params.clone(),
            );
        }
        let response = host_bridge.request_until(method, Some(params), timeout, is_cancelled)?;
        if let Some(trace) = &trace {
            trace.record_value(
                "client_to_agent",
                &format!("{method}.response"),
                response.clone(),
            );
        }
        if response.is_null() {
            return serde_json::from_value(serde_json::json!({}))
                .map_err(|error| RuntimeError::Internal(error.to_string()));
        }
        serde_json::from_value(response).map_err(|error| RuntimeError::Internal(error.to_string()))
    })
    .await
    .map_err(|error| RuntimeError::Internal(error.to_string()))?
}
