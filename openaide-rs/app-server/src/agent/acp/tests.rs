use super::*;
use crate::agent::acp_agent_status::agent_probe_result_from_initialize;
use crate::agent::acp_host::{
    host_request, initialize_request, read_text_file_from_host, write_text_file_from_host,
};
use crate::agent::acp_runtime_threading::close_in_parallel;
use crate::agent::acp_session_capabilities::{
    validate_auth_method, validate_initialize_protocol, validate_session_list_capability,
};
use crate::agent::acp_session_catalogs::{
    attach_session_event_sink_to_slot, deliver_session_commands_catalog,
    deliver_session_config_catalog, deliver_session_metadata_update,
    session_catalogs_from_dispatch, PendingSessionCatalogs,
};
use crate::agent::acp_session_lifecycle::{
    agent_list_sessions_result_from_response, initialize_supports_session_close,
    initialize_supports_session_delete, load_active_session, request_session_list,
    start_active_session, validate_load_session_capability, LoadActiveSessionRequest,
    LoadReplayCapture,
};
use crate::agent::acp_session_paths::normalized_session_cwd;
use crate::agent::acp_update_projection::{LivePromptProjection, ReplayProjection};
use crate::agent::events::{
    AgentEvent, AgentPermissionOutcome, AgentPermissionRequest, AgentToolCallStatus,
};
use crate::agent::prompt_content::{
    build_prompt_content_with_policy, PromptContentCapabilities, PromptContentPolicy,
};
use crate::agent::tool_details::tool_call_event;
use crate::agent::{AgentMetadataField, AgentSessionMetadataUpdate};
use crate::protocol::model::{
    ActivityStatus, ActivityToolContent, AgentCommandsCatalog, Attachment, ConfigOption,
    ConfigOptionCategory, ConfigOptionValue, ConfigOptionsStatus, NormalizedMessage,
};
use agent_client_protocol::schema::{
    AgentCapabilities, AuthMethod, AuthMethodAgent, AuthenticateRequest, AuthenticateResponse,
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, ContentBlock, ContentChunk,
    CreateTerminalRequest, CreateTerminalResponse, Diff, Implementation, InitializeRequest,
    InitializeResponse, ListSessionsRequest, ListSessionsResponse, LoadSessionRequest,
    LoadSessionResponse, McpCapabilities, NewSessionRequest, NewSessionResponse, PermissionOption,
    PermissionOptionKind, PromptCapabilities, ProtocolVersion, ReadTextFileRequest,
    RequestPermissionOutcome, RequestPermissionRequest, SessionCapabilities,
    SessionCloseCapabilities, SessionConfigOption,
    SessionConfigOptionCategory as AcpConfigOptionCategory, SessionConfigSelectOption,
    SessionDeleteCapabilities, SessionInfo, SessionInfoUpdate, SessionListCapabilities,
    SessionNotification, SessionUpdate, TextContent, ToolCall, ToolCallContent, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind, UnstructuredCommandInput,
    WaitForTerminalExitRequest, WaitForTerminalExitResponse, WriteTextFileRequest,
};
use agent_client_protocol::JsonRpcMessage;
use agent_client_protocol::{Agent, Client, ConnectionTo, Handled};
use std::collections::HashMap;
use std::env;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

mod active_session_runtime;
mod task_chat_runtime;

#[derive(Default)]
struct CapturingSessionSink {
    events: Mutex<Vec<AgentEvent>>,
    catalogs: Mutex<Vec<ConfigOptionsCatalog>>,
    command_catalogs: Mutex<Vec<AgentCommandsCatalog>>,
    metadata_updates: Mutex<Vec<AgentSessionMetadataUpdate>>,
}

#[derive(Default)]
struct CapturingEventSink {
    events: Mutex<Vec<AgentEvent>>,
    permissions: Mutex<Vec<AgentPermissionRequest>>,
}

#[derive(Clone)]
struct AuthRequiredTestAgent {
    authenticated: Arc<AtomicBool>,
    authenticate_count: Arc<AtomicUsize>,
    new_session_count: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct SessionListAuthTestAgent {
    authenticated: Arc<AtomicBool>,
    authenticate_count: Arc<AtomicUsize>,
    list_count: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct LoadSessionReplayTestAgent;

impl agent_client_protocol::ConnectTo<Client> for AuthRequiredTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        let authenticated = self.authenticated.clone();
        let authenticate_count = self.authenticate_count.clone();
        let new_session_authenticated = self.authenticated.clone();
        let new_session_count = self.new_session_count.clone();

        Agent
            .builder()
            .name("auth-required-test-agent")
            .on_receive_request(
                async move |_request: InitializeRequest, responder, _connection| {
                    responder.respond(InitializeResponse::new(ProtocolVersion::V1).auth_methods(
                        vec![AuthMethod::Agent(AuthMethodAgent::new(
                            "codex-login",
                            "Codex login",
                        ))],
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: AuthenticateRequest, responder, _connection| {
                    assert_eq!(request.method_id.0.as_ref(), "codex-login");
                    authenticate_count.fetch_add(1, Ordering::SeqCst);
                    authenticated.store(true, Ordering::SeqCst);
                    responder.respond(AuthenticateResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, _connection| {
                    new_session_count.fetch_add(1, Ordering::SeqCst);
                    if new_session_authenticated.load(Ordering::SeqCst) {
                        responder.respond(NewSessionResponse::new("session-authenticated"))
                    } else {
                        responder.respond_with_error(agent_client_protocol::Error::auth_required())
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(client)
    }
}

impl agent_client_protocol::ConnectTo<Client> for SessionListAuthTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        let authenticated = self.authenticated.clone();
        let authenticate_count = self.authenticate_count.clone();
        let list_authenticated = self.authenticated.clone();
        let list_count = self.list_count.clone();

        Agent
            .builder()
            .name("session-list-auth-test-agent")
            .on_receive_request(
                async move |_request: InitializeRequest, responder, _connection| {
                    responder.respond(
                        InitializeResponse::new(ProtocolVersion::V1)
                            .agent_capabilities(AgentCapabilities::new().session_capabilities(
                                SessionCapabilities::new().list(SessionListCapabilities::new()),
                            ))
                            .auth_methods(vec![AuthMethod::Agent(AuthMethodAgent::new(
                                "codex-login",
                                "Codex login",
                            ))]),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: AuthenticateRequest, responder, _connection| {
                    assert_eq!(request.method_id.0.as_ref(), "codex-login");
                    authenticate_count.fetch_add(1, Ordering::SeqCst);
                    authenticated.store(true, Ordering::SeqCst);
                    responder.respond(AuthenticateResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: ListSessionsRequest, responder, _connection| {
                    list_count.fetch_add(1, Ordering::SeqCst);
                    if list_authenticated.load(Ordering::SeqCst) {
                        let cwd = request.cwd.unwrap_or_else(|| PathBuf::from("/workspace"));
                        responder.respond(ListSessionsResponse::new(vec![SessionInfo::new(
                            "session-listed",
                            cwd,
                        )]))
                    } else {
                        responder.respond_with_error(agent_client_protocol::Error::auth_required())
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(client)
    }
}

impl agent_client_protocol::ConnectTo<Client> for LoadSessionReplayTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        Agent
            .builder()
            .name("load-session-replay-test-agent")
            .on_receive_request(
                async move |_request: InitializeRequest, responder, _connection| {
                    responder.respond(
                        InitializeResponse::new(ProtocolVersion::V1)
                            .agent_capabilities(AgentCapabilities::new().load_session(true)),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: LoadSessionRequest, responder, connection| {
                    assert_eq!(request.session_id.to_string(), "external-session");
                    assert!(request.cwd.is_absolute());
                    connection.send_notification(SessionNotification::new(
                        request.session_id.clone(),
                        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new("Prior user question"),
                        ))),
                    ))?;
                    connection.send_notification(SessionNotification::new(
                        request.session_id.clone(),
                        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(vec![
                            AvailableCommand::new("web", "Search the web").input(
                                AvailableCommandInput::Unstructured(UnstructuredCommandInput::new(
                                    "query",
                                )),
                            ),
                        ])),
                    ))?;
                    connection.send_notification(SessionNotification::new(
                        request.session_id,
                        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new("Prior agent answer"),
                        ))),
                    ))?;
                    responder.respond(LoadSessionResponse::new().config_options(vec![
                            SessionConfigOption::select(
                                "model",
                                "Model",
                                "gpt-5.5",
                                vec![SessionConfigSelectOption::new("gpt-5.5", "GPT-5.5")],
                            )
                            .category(AcpConfigOptionCategory::Model),
                        ]))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(client)
    }
}

impl CapturingEventSink {
    fn events(&self) -> Vec<AgentEvent> {
        self.events
            .lock()
            .expect("captured event lock poisoned")
            .clone()
    }

    fn permissions(&self) -> Vec<AgentPermissionRequest> {
        self.permissions
            .lock()
            .expect("captured permission lock poisoned")
            .clone()
    }
}

impl AgentEventSink for CapturingEventSink {
    fn emit(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        self.events
            .lock()
            .expect("captured event lock poisoned")
            .push(event);
        Ok(())
    }

    fn request_permission(
        &self,
        request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        self.permissions
            .lock()
            .expect("captured permission lock poisoned")
            .push(request);
        Ok(AgentPermissionOutcome::Cancelled)
    }
}

fn session_update_dispatch(update: SessionUpdate) -> agent_client_protocol::Dispatch {
    agent_client_protocol::Dispatch::Notification(
        SessionNotification::new("session_1", update)
            .to_untyped_message()
            .unwrap(),
    )
}

#[test]
fn initialize_advertises_client_methods_only_when_host_bridge_is_enabled() {
    let disabled = initialize_request(&HostBridge::disabled());
    assert!(!disabled.client_capabilities.fs.read_text_file);
    assert!(!disabled.client_capabilities.fs.write_text_file);
    assert!(!disabled.client_capabilities.terminal);

    let (bridge, _requests) = HostBridge::channel();
    let enabled = initialize_request(&bridge);
    assert!(enabled.client_capabilities.fs.read_text_file);
    assert!(enabled.client_capabilities.fs.write_text_file);
    assert!(enabled.client_capabilities.terminal);
}

#[test]
fn read_text_file_request_round_trips_through_host_bridge() {
    let (bridge, requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));
    let request_bridge = bridge.clone();
    let pending = std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(read_text_file_from_host(
                request_bridge,
                ReadTextFileRequest::new("session_1", "/workspace/app/src/main.rs")
                    .line(2)
                    .limit(3),
                None,
            ))
    });

    let outbound = requests
        .recv_timeout(Duration::from_secs(1))
        .expect("read request should be sent to host");
    assert_eq!(outbound.method, "fs/read_text_file");
    assert_eq!(outbound.params.as_ref().unwrap()["sessionId"], "session_1");
    assert_eq!(
        outbound.params.as_ref().unwrap()["path"],
        "/workspace/app/src/main.rs"
    );
    assert_eq!(outbound.params.as_ref().unwrap()["line"], 2);
    assert_eq!(outbound.params.as_ref().unwrap()["limit"], 3);

    assert!(bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": outbound.id,
        "result": { "content": "line 2\n" }
    })));
    assert_eq!(
        pending
            .join()
            .expect("host read thread should finish")
            .unwrap()
            .content,
        "line 2\n"
    );
}

#[test]
fn write_text_file_request_accepts_null_host_response() {
    let (bridge, requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));
    let request_bridge = bridge.clone();
    let pending = std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(write_text_file_from_host(
                request_bridge,
                WriteTextFileRequest::new("session_1", "/workspace/app/src/main.rs", "updated\n"),
                None,
            ))
    });

    let outbound = requests
        .recv_timeout(Duration::from_secs(1))
        .expect("write request should be sent to host");
    assert_eq!(outbound.method, "fs/write_text_file");
    assert_eq!(outbound.params.as_ref().unwrap()["content"], "updated\n");

    assert!(bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": outbound.id,
        "result": null
    })));
    pending
        .join()
        .expect("host write thread should finish")
        .unwrap();
}

#[test]
fn terminal_create_request_round_trips_through_host_bridge() {
    let (bridge, requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));
    let request_bridge = bridge.clone();
    let pending = std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(host_request::<_, CreateTerminalResponse>(
                request_bridge,
                "terminal/create",
                CreateTerminalRequest::new("session_1", "npm")
                    .args(vec!["test".to_string()])
                    .cwd(Some(PathBuf::from("/workspace/app"))),
                Some(Duration::from_secs(1)),
                || false,
                None,
            ))
    });

    let outbound = requests
        .recv_timeout(Duration::from_secs(1))
        .expect("terminal create request should be sent to host");
    assert_eq!(outbound.method, "terminal/create");
    assert_eq!(outbound.params.as_ref().unwrap()["command"], "npm");
    assert_eq!(outbound.params.as_ref().unwrap()["args"][0], "test");

    assert!(bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": outbound.id,
        "result": { "terminalId": "term_1" }
    })));
    assert_eq!(
        pending
            .join()
            .expect("terminal create thread should finish")
            .unwrap()
            .terminal_id
            .to_string(),
        "term_1"
    );
}

#[test]
fn terminal_wait_request_can_be_cancelled_without_deadline() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let (bridge, requests) = HostBridge::channel_with_timeout(Duration::from_millis(1));
    let cancelled = Arc::new(AtomicBool::new(false));
    let request_bridge = bridge.clone();
    let request_cancelled = cancelled.clone();
    let pending = std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(host_request::<_, WaitForTerminalExitResponse>(
                request_bridge,
                "terminal/wait_for_exit",
                WaitForTerminalExitRequest::new("session_1", "term_1"),
                None,
                move || request_cancelled.load(Ordering::SeqCst),
                None,
            ))
    });

    let outbound = requests
        .recv_timeout(Duration::from_secs(1))
        .expect("terminal wait request should be sent to host");
    assert_eq!(outbound.method, "terminal/wait_for_exit");
    cancelled.store(true, Ordering::SeqCst);

    let error = pending
        .join()
        .expect("terminal wait thread should finish")
        .expect_err("terminal wait should be cancelled");
    assert!(matches!(error, RuntimeError::NotReady(_)));
}

impl CapturingSessionSink {
    fn events(&self) -> Vec<AgentEvent> {
        self.events
            .lock()
            .expect("captured session event lock poisoned")
            .clone()
    }

    fn current_values(&self) -> Vec<String> {
        self.catalogs
            .lock()
            .expect("captured catalog lock poisoned")
            .iter()
            .filter_map(|catalog| catalog.options.first())
            .map(|option| option.current_value.clone())
            .collect()
    }

    fn command_names(&self) -> Vec<String> {
        self.command_catalogs
            .lock()
            .expect("captured command catalog lock poisoned")
            .iter()
            .flat_map(|catalog| catalog.commands.iter())
            .map(|command| command.name.clone())
            .collect()
    }

    fn metadata_updates(&self) -> Vec<AgentSessionMetadataUpdate> {
        self.metadata_updates
            .lock()
            .expect("captured session metadata lock poisoned")
            .clone()
    }
}

impl AgentSessionEventSink for CapturingSessionSink {
    fn session_update(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        self.events
            .lock()
            .expect("captured session event lock poisoned")
            .push(event);
        Ok(())
    }

    fn config_options_changed(&self, catalog: ConfigOptionsCatalog) -> Result<(), RuntimeError> {
        self.catalogs
            .lock()
            .expect("captured catalog lock poisoned")
            .push(catalog);
        Ok(())
    }

    fn commands_changed(&self, catalog: AgentCommandsCatalog) -> Result<(), RuntimeError> {
        self.command_catalogs
            .lock()
            .expect("captured command catalog lock poisoned")
            .push(catalog);
        Ok(())
    }

    fn metadata_changed(&self, update: AgentSessionMetadataUpdate) -> Result<(), RuntimeError> {
        self.metadata_updates
            .lock()
            .expect("captured session metadata lock poisoned")
            .push(update);
        Ok(())
    }
}

#[tokio::test]
async fn session_info_dispatch_preserves_agent_title_patch() {
    let dispatch = session_update_dispatch(SessionUpdate::SessionInfoUpdate(
        SessionInfoUpdate::new().title("Agent generated title"),
    ));

    let updates = session_catalogs_from_dispatch("codex", dispatch)
        .await
        .expect("project session metadata");

    assert_eq!(
        updates.metadata,
        Some(AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Agent generated title".to_string()),
            updated_at: AgentMetadataField::Unchanged,
        })
    );

    let cleared = session_catalogs_from_dispatch(
        "codex",
        session_update_dispatch(SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new().title(None::<String>),
        )),
    )
    .await
    .expect("project cleared session title");
    assert_eq!(
        cleared.metadata,
        Some(AgentSessionMetadataUpdate {
            title: AgentMetadataField::Clear,
            updated_at: AgentMetadataField::Unchanged,
        })
    );

    let omitted = session_catalogs_from_dispatch(
        "codex",
        session_update_dispatch(SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new())),
    )
    .await
    .expect("project omitted session title");
    assert_eq!(
        omitted.metadata,
        Some(AgentSessionMetadataUpdate::default())
    );
}

#[test]
fn session_metadata_buffers_and_merges_partial_updates_until_sink_attaches() {
    let mut session_event_sink = None;
    let mut pending_catalogs = PendingSessionCatalogs::default();
    deliver_session_metadata_update(
        AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Agent title".to_string()),
            updated_at: AgentMetadataField::Unchanged,
        },
        session_event_sink.as_ref(),
        &mut pending_catalogs,
    )
    .unwrap();
    deliver_session_metadata_update(
        AgentSessionMetadataUpdate {
            title: AgentMetadataField::Unchanged,
            updated_at: AgentMetadataField::Value("2026-07-10T10:00:00Z".to_string()),
        },
        session_event_sink.as_ref(),
        &mut pending_catalogs,
    )
    .unwrap();

    let sink = Arc::new(CapturingSessionSink::default());
    attach_session_event_sink_to_slot(&mut session_event_sink, &mut pending_catalogs, sink.clone())
        .unwrap();

    assert_eq!(
        sink.metadata_updates(),
        vec![AgentSessionMetadataUpdate {
            title: AgentMetadataField::Value("Agent title".to_string()),
            updated_at: AgentMetadataField::Value("2026-07-10T10:00:00Z".to_string()),
        }]
    );
}

#[test]
fn session_config_catalog_buffers_latest_update_until_sink_attaches() {
    let mut session_event_sink = None;
    let mut pending_catalogs = PendingSessionCatalogs::default();

    deliver_session_config_catalog(
        config_catalog("ask"),
        session_event_sink.as_ref(),
        &mut pending_catalogs,
    )
    .unwrap();
    deliver_session_config_catalog(
        config_catalog("code"),
        session_event_sink.as_ref(),
        &mut pending_catalogs,
    )
    .unwrap();

    let sink = Arc::new(CapturingSessionSink::default());
    attach_session_event_sink_to_slot(&mut session_event_sink, &mut pending_catalogs, sink.clone())
        .unwrap();

    assert_eq!(sink.current_values(), vec!["code"]);
}

#[test]
fn session_config_catalog_delivers_immediately_when_sink_is_attached() {
    let sink = Arc::new(CapturingSessionSink::default());
    let mut session_event_sink: Option<Arc<dyn AgentSessionEventSink>> = Some(sink.clone());
    let mut pending_catalogs = PendingSessionCatalogs::default();

    deliver_session_config_catalog(
        config_catalog("ask"),
        session_event_sink.as_ref(),
        &mut pending_catalogs,
    )
    .unwrap();

    assert_eq!(sink.current_values(), vec!["ask"]);
    session_event_sink.take();
}

#[test]
fn session_command_catalog_buffers_latest_update_until_sink_attaches() {
    let mut session_event_sink = None;
    let mut pending_catalogs = PendingSessionCatalogs::default();

    deliver_session_commands_catalog(
        command_catalog("old"),
        session_event_sink.as_ref(),
        &mut pending_catalogs,
    )
    .unwrap();
    deliver_session_commands_catalog(
        command_catalog("web"),
        session_event_sink.as_ref(),
        &mut pending_catalogs,
    )
    .unwrap();

    let sink = Arc::new(CapturingSessionSink::default());
    attach_session_event_sink_to_slot(&mut session_event_sink, &mut pending_catalogs, sink.clone())
        .unwrap();

    assert_eq!(sink.command_names(), vec!["web"]);
}

#[test]
fn initialize_close_capability_requires_explicit_session_close_support() {
    let without_close = InitializeResponse::new(ProtocolVersion::V1);
    assert!(!initialize_supports_session_close(&without_close));

    let with_close = InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(
        AgentCapabilities::new().session_capabilities(
            SessionCapabilities::new().close(SessionCloseCapabilities::new()),
        ),
    );
    assert!(initialize_supports_session_close(&with_close));
}

#[test]
fn initialize_delete_capability_requires_explicit_session_delete_support() {
    let without_delete = InitializeResponse::new(ProtocolVersion::V1);
    assert!(!initialize_supports_session_delete(&without_delete));

    let with_delete = InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(
        AgentCapabilities::new().session_capabilities(
            SessionCapabilities::new().delete(SessionDeleteCapabilities::new()),
        ),
    );
    assert!(initialize_supports_session_delete(&with_delete));
}

#[test]
fn initialize_protocol_rejects_unsupported_major_version() {
    let unsupported = InitializeResponse::new(ProtocolVersion::from(2_u16));

    let error = validate_initialize_protocol(&unsupported).unwrap_err();

    assert!(matches!(error, RuntimeError::Unsupported(_)));
    assert!(error
        .to_string()
        .contains("unsupported ACP protocol version 2"));
}

#[test]
fn probe_result_normalizes_initialize_without_raw_payloads() {
    let initialize = InitializeResponse::new(ProtocolVersion::V1)
        .agent_info(Implementation::new("codex-acp", "1.2.3").title("Codex ACP"))
        .agent_capabilities(
            AgentCapabilities::new()
                .load_session(true)
                .prompt_capabilities(PromptCapabilities::new().image(true).embedded_context(true))
                .mcp_capabilities(McpCapabilities::new().http(true))
                .session_capabilities(
                    SessionCapabilities::new().close(SessionCloseCapabilities::new()),
                ),
        )
        .auth_methods(vec![AuthMethod::Agent(
            AuthMethodAgent::new("codex-login", "Codex login")
                .description("Sign in with the Agent."),
        )]);

    let result = agent_probe_result_from_initialize("codex".to_string(), &initialize);

    assert_eq!(result.agent_id, "codex");
    assert_eq!(result.protocol_version, "1");
    assert_eq!(result.implementation_name.as_deref(), Some("Codex ACP"));
    assert_eq!(result.implementation_version.as_deref(), Some("1.2.3"));
    assert!(result.capabilities.contains(&"Load sessions".to_string()));
    assert!(result.capabilities.contains(&"Image prompts".to_string()));
    assert!(result
        .capabilities
        .contains(&"Embedded context".to_string()));
    assert!(result.capabilities.contains(&"HTTP MCP".to_string()));
    assert!(result.capabilities.contains(&"Close sessions".to_string()));
    assert_eq!(result.auth_methods[0].id, "codex-login");
    assert_eq!(result.auth_methods[0].kind, "agent");
}

#[test]
fn validate_auth_method_accepts_agent_methods_only() {
    let initialize =
        InitializeResponse::new(ProtocolVersion::V1).auth_methods(vec![AuthMethod::Agent(
            AuthMethodAgent::new("codex-login", "Codex login"),
        )]);

    validate_auth_method(&initialize, "codex-login").unwrap();
    assert!(matches!(
        validate_auth_method(&initialize, "missing").unwrap_err(),
        RuntimeError::InvalidParams(_)
    ));
}

#[test]
fn session_list_capability_requires_explicit_support() {
    let without_list = InitializeResponse::new(ProtocolVersion::V1);
    assert!(matches!(
        validate_session_list_capability(&without_list).unwrap_err(),
        RuntimeError::CapabilityMissing(_)
    ));

    let with_list = InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(
        AgentCapabilities::new()
            .session_capabilities(SessionCapabilities::new().list(SessionListCapabilities::new())),
    );
    validate_session_list_capability(&with_list).unwrap();
}

#[test]
fn load_session_capability_requires_explicit_support() {
    let without_load = InitializeResponse::new(ProtocolVersion::V1);
    assert!(matches!(
        validate_load_session_capability(&without_load).unwrap_err(),
        RuntimeError::CapabilityMissing(_)
    ));

    let with_load = InitializeResponse::new(ProtocolVersion::V1)
        .agent_capabilities(AgentCapabilities::new().load_session(true));
    validate_load_session_capability(&with_load).unwrap();
}

#[test]
fn replayed_session_updates_are_normalized_as_chat_history() {
    let messages = ReplayProjection::new("session-replay").project(vec![
        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "Prior user question",
        )))),
        SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "private streamed thought",
        )))),
        SessionUpdate::ToolCall(
            ToolCall::new("tool_call_1", "Read file")
                .kind(ToolKind::Read)
                .status(ToolCallStatus::InProgress),
        ),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool_call_1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .content(vec![ToolCallContent::from(ContentBlock::Text(
                    TextContent::new("Read file output"),
                ))]),
        )),
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "Prior agent answer",
        )))),
    ]);

    assert_eq!(messages.len(), 4);
    match &messages[0] {
        NormalizedMessage::User {
            text, attachments, ..
        } => {
            assert_eq!(text, "Prior user question");
            assert!(attachments.is_empty());
        }
        other => panic!("expected user replay, got {other:?}"),
    }
    match &messages[1] {
        NormalizedMessage::Thought { text, .. } => {
            assert_eq!(text, "private streamed thought");
        }
        other => panic!("expected thought replay, got {other:?}"),
    }
    match &messages[2] {
        NormalizedMessage::Activity {
            title,
            status,
            steps,
            ..
        } => {
            assert_eq!(title, "Read file");
            assert_eq!(status, &ActivityStatus::Completed);
            assert_eq!(steps.len(), 1);
        }
        other => panic!("expected completed tool replay, got {other:?}"),
    }
    match &messages[3] {
        NormalizedMessage::AgentText { text, .. } => {
            assert_eq!(text, "Prior agent answer");
        }
        other => panic!("expected agent replay, got {other:?}"),
    }
}

#[test]
fn replayed_text_without_source_ids_has_restart_stable_identity() {
    let updates = || {
        vec![SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("Stable replay")),
        ))]
    };

    let first = ReplayProjection::new("session-stable").project(updates());
    let second = ReplayProjection::new("session-stable").project(updates());

    assert_eq!(first[0].identity(), second[0].identity());
    assert!(first[0]
        .identity()
        .starts_with("acp:session-stable:replay:agent:"));
}

#[test]
fn replay_continues_a_sourced_message_across_tool_activity() {
    let messages = ReplayProjection::new("session-source-tool-source").project(vec![
        SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new("Before")))
                .message_id("agent-message-1".to_string()),
        ),
        SessionUpdate::ToolCall(
            ToolCall::new("tool_call_1", "Read file")
                .kind(ToolKind::Read)
                .status(ToolCallStatus::InProgress),
        ),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool_call_1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )),
        SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new(" after")))
                .message_id("agent-message-1".to_string()),
        ),
    ]);

    assert_eq!(messages.len(), 2);
    match &messages[0] {
        NormalizedMessage::AgentText { id, text, .. } => {
            assert_eq!(id, "acp:session-source-tool-source:message:agent-message-1");
            assert_eq!(text, "Before after");
        }
        other => panic!("expected one sourced message, got {other:?}"),
    }
    assert!(matches!(
        &messages[1],
        NormalizedMessage::Activity {
            status: ActivityStatus::Completed,
            ..
        }
    ));
}

#[test]
fn replay_continues_each_sourced_message_across_interleaved_messages() {
    let messages = ReplayProjection::new("session-interleaved-sources").project(vec![
        SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new("First")))
                .message_id("agent-message-a".to_string()),
        ),
        SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new("Second")))
                .message_id("agent-message-b".to_string()),
        ),
        SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new(" continued")))
                .message_id("agent-message-a".to_string()),
        ),
    ]);

    assert_eq!(messages.len(), 2);
    match messages.as_slice() {
        [NormalizedMessage::AgentText {
            id: first_id,
            text: first_text,
            ..
        }, NormalizedMessage::AgentText {
            id: second_id,
            text: second_text,
            ..
        }] => {
            assert_eq!(
                first_id,
                "acp:session-interleaved-sources:message:agent-message-a"
            );
            assert_eq!(first_text, "First continued");
            assert_eq!(
                second_id,
                "acp:session-interleaved-sources:message:agent-message-b"
            );
            assert_eq!(second_text, "Second");
        }
        other => panic!("expected one row per sourced message, got {other:?}"),
    }
}

#[test]
fn replay_ends_anonymous_text_at_tool_activity() {
    let messages = ReplayProjection::new("session-anonymous-tool-boundary").project(vec![
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "Before",
        )))),
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            " tool",
        )))),
        SessionUpdate::ToolCall(
            ToolCall::new("tool_call_1", "Read file")
                .kind(ToolKind::Read)
                .status(ToolCallStatus::Completed),
        ),
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "After tool",
        )))),
    ]);

    assert_eq!(messages.len(), 3);
    assert!(matches!(
        &messages[0],
        NormalizedMessage::AgentText { text, .. } if text == "Before tool"
    ));
    assert!(matches!(&messages[1], NormalizedMessage::Activity { .. }));
    assert!(matches!(
        &messages[2],
        NormalizedMessage::AgentText { text, .. } if text == "After tool"
    ));
    assert_ne!(messages[0].identity(), messages[2].identity());
}

#[test]
fn replay_keeps_anonymous_text_distinct_when_the_next_chunk_has_a_source_id() {
    let updates = || {
        vec![
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new("Anonymous message"),
            ))),
            SessionUpdate::AgentMessageChunk(
                ContentChunk::new(ContentBlock::Text(TextContent::new("Sourced message")))
                    .message_id("source-message-1".to_string()),
            ),
        ]
    };

    let first = ReplayProjection::new("session-identity-boundary").project(updates());
    let second = ReplayProjection::new("session-identity-boundary").project(updates());

    assert_eq!(first.len(), 2);
    assert_eq!(
        first
            .iter()
            .map(NormalizedMessage::identity)
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(NormalizedMessage::identity)
            .collect::<Vec<_>>()
    );
    match first.as_slice() {
        [NormalizedMessage::AgentText {
            id: anonymous_id,
            text: anonymous_text,
            ..
        }, NormalizedMessage::AgentText {
            id: sourced_id,
            text: sourced_text,
            ..
        }] => {
            assert!(anonymous_id.starts_with("acp:session-identity-boundary:replay:agent:"));
            assert_eq!(anonymous_text, "Anonymous message");
            assert_eq!(
                sourced_id,
                "acp:session-identity-boundary:message:source-message-1"
            );
            assert_eq!(sourced_text, "Sourced message");
        }
        other => panic!("expected distinct anonymous and sourced messages, got {other:?}"),
    }
}

#[test]
fn replay_keeps_sourced_text_distinct_when_the_next_chunk_is_anonymous() {
    let updates = || {
        vec![
            SessionUpdate::AgentMessageChunk(
                ContentChunk::new(ContentBlock::Text(TextContent::new("Sourced message")))
                    .message_id("source-message-1".to_string()),
            ),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new("Anonymous message"),
            ))),
        ]
    };

    let first = ReplayProjection::new("session-reverse-identity-boundary").project(updates());
    let second = ReplayProjection::new("session-reverse-identity-boundary").project(updates());

    assert_eq!(first.len(), 2);
    assert_eq!(
        first
            .iter()
            .map(NormalizedMessage::identity)
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(NormalizedMessage::identity)
            .collect::<Vec<_>>()
    );
    match first.as_slice() {
        [NormalizedMessage::AgentText {
            id: sourced_id,
            text: sourced_text,
            ..
        }, NormalizedMessage::AgentText {
            id: anonymous_id,
            text: anonymous_text,
            ..
        }] => {
            assert_eq!(
                sourced_id,
                "acp:session-reverse-identity-boundary:message:source-message-1"
            );
            assert_eq!(sourced_text, "Sourced message");
            assert!(anonymous_id.starts_with("acp:session-reverse-identity-boundary:replay:agent:"));
            assert_eq!(anonymous_text, "Anonymous message");
        }
        other => panic!("expected distinct sourced and anonymous messages, got {other:?}"),
    }
}

#[test]
fn live_agent_thought_chunks_emit_thought_events_not_tool_activity() {
    let capture = Arc::new(CapturingEventSink::default());
    let sink: Arc<dyn AgentEventSink> = capture.clone();
    let projection =
        LivePromptProjection::new("codex", sink, crate::agent::TurnCancellation::new());

    projection
        .emit(SessionUpdate::AgentThoughtChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("one")),
        )))
        .unwrap();
    projection
        .emit(SessionUpdate::AgentThoughtChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(" more")),
        )))
        .unwrap();

    let events = capture.events();
    assert_eq!(events.len(), 2);
    match &events[0] {
        AgentEvent::ThoughtChunk { text, .. } => assert_eq!(text, "one"),
        other => panic!("expected thought event, got {other:?}"),
    }
    match &events[1] {
        AgentEvent::ThoughtChunk { text, .. } => assert_eq!(text, " more"),
        other => panic!("expected thought event, got {other:?}"),
    }
}

#[test]
fn session_list_result_is_workspace_scoped_and_normalized() {
    let requested_cwd = PathBuf::from("/workspace/app");
    let other_cwd = PathBuf::from("/workspace/other");
    let response = ListSessionsResponse::new(vec![
        SessionInfo::new("prepared-session", requested_cwd.clone()).title("Prepared"),
        SessionInfo::new("session-one", requested_cwd.clone())
            .title("Implement list")
            .updated_at("2026-05-18T10:00:00Z"),
        SessionInfo::new("session-two", other_cwd).title("Ignore other project"),
    ])
    .next_cursor("opaque-cursor");

    let result = agent_list_sessions_result_from_response(
        "codex".to_string(),
        response,
        &requested_cwd,
        Some("prepared-session"),
    );

    assert_eq!(result.agent_id, "codex");
    assert_eq!(result.next_cursor.as_deref(), Some("opaque-cursor"));
    assert_eq!(result.sessions.len(), 1);
    assert_eq!(result.sessions[0].session_id, "session-one");
    assert_eq!(result.sessions[0].cwd, "/workspace/app");
    assert_eq!(result.sessions[0].title.as_deref(), Some("Implement list"));
    assert_eq!(
        result.sessions[0].last_activity.as_deref(),
        Some("2026-05-18T10:00:00Z")
    );
    assert_eq!(
        result.sessions[0].updated_at.as_deref(),
        Some("2026-05-18T10:00:00Z")
    );
}

#[test]
fn start_active_session_retries_auth_required_on_same_connection() {
    let authenticated = Arc::new(AtomicBool::new(false));
    let authenticate_count = Arc::new(AtomicUsize::new(0));
    let new_session_count = Arc::new(AtomicUsize::new(0));
    let agent = AuthRequiredTestAgent {
        authenticated,
        authenticate_count: authenticate_count.clone(),
        new_session_count: new_session_count.clone(),
    };

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        Client
            .builder()
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                let initialize = connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let (session, _options) = start_active_session(
                    &connection,
                    env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
                    &initialize,
                    Some("codex-login"),
                    None,
                )
                .await?;

                assert_eq!(session.session_id().to_string(), "session-authenticated");
                Ok(())
            })
            .await
            .unwrap();
    });

    assert_eq!(authenticate_count.load(Ordering::SeqCst), 1);
    assert_eq!(new_session_count.load(Ordering::SeqCst), 2);
}

#[test]
fn session_list_retries_auth_required_on_same_connection() {
    let authenticated = Arc::new(AtomicBool::new(false));
    let authenticate_count = Arc::new(AtomicUsize::new(0));
    let list_count = Arc::new(AtomicUsize::new(0));
    let requested_cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let agent = SessionListAuthTestAgent {
        authenticated,
        authenticate_count: authenticate_count.clone(),
        list_count: list_count.clone(),
    };

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        Client
            .builder()
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                let initialize = connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let response = request_session_list(
                    &connection,
                    requested_cwd.clone(),
                    None,
                    &initialize,
                    Some("codex-login"),
                )
                .await?;

                let result = agent_list_sessions_result_from_response(
                    "codex".to_string(),
                    response,
                    &requested_cwd,
                    None,
                );
                assert_eq!(result.sessions[0].session_id, "session-listed");
                Ok(())
            })
            .await
            .unwrap();
    });

    assert_eq!(authenticate_count.load(Ordering::SeqCst), 1);
    assert_eq!(list_count.load(Ordering::SeqCst), 2);
}

#[test]
fn load_active_session_captures_replayed_updates_before_response() {
    let requested_cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let load_replay: Arc<Mutex<HashMap<String, LoadReplayCapture>>> = Arc::default();
    let notification_load_replay = load_replay.clone();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        Client
            .builder()
            .on_receive_notification(
                async move |notification: SessionNotification, cx| {
                    let mut active = notification_load_replay
                        .lock()
                        .expect("ACP load replay capture lock poisoned");
                    if let Some(capture) = active.get_mut(&notification.session_id.to_string()) {
                        if notification.session_id == capture.session_id {
                            capture.updates.push(notification.update);
                            return Ok(Handled::Yes);
                        }
                    }
                    Ok(Handled::No {
                        message: (notification, cx),
                        retry: false,
                    })
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(
                LoadSessionReplayTestAgent,
                |connection: ConnectionTo<Agent>| async move {
                    let initialize = connection
                        .send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    let (active_session, catalog, command_catalog, replayed_messages) =
                        load_active_session(
                            &connection,
                            &initialize,
                            &load_replay,
                            None,
                            LoadActiveSessionRequest {
                                agent_id: "codex",
                                session_id: "external-session".to_string(),
                                cwd: requested_cwd,
                                preferred_auth_method_id: None,
                            },
                        )
                        .await
                        .map_err(|error| {
                            agent_client_protocol::util::internal_error(error.to_string())
                        })?;

                    assert_eq!(active_session.session_id().to_string(), "external-session");
                    assert_eq!(catalog.model_id().as_deref(), Some("gpt-5.5"));
                    let command_catalog = command_catalog.expect("replayed command catalog");
                    assert_eq!(command_catalog.commands[0].name, "web");
                    assert_eq!(
                        command_catalog.commands[0].input_hint.as_deref(),
                        Some("query")
                    );
                    assert_eq!(replayed_messages.len(), 2);
                    match &replayed_messages[0] {
                        NormalizedMessage::User { text, .. } => {
                            assert_eq!(text, "Prior user question");
                        }
                        other => panic!("expected replayed user message, got {other:?}"),
                    }
                    match &replayed_messages[1] {
                        NormalizedMessage::AgentText { text, .. } => {
                            assert_eq!(text, "Prior agent answer");
                        }
                        other => panic!("expected replayed agent message, got {other:?}"),
                    }
                    Ok(())
                },
            )
            .await
            .unwrap();
    });
}

#[cfg(unix)]
#[test]
fn probe_timeout_cancels_hanging_agent_process() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let pid_file = temp.path().join("agent.pid");
    let runtime = AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "printf '%s' $$ > \"$PID_FILE\"; sleep 30".to_string(),
        ],
        env: vec![(
            "PID_FILE".to_string(),
            pid_file.to_string_lossy().to_string(),
        )],
        secret_env: Vec::new(),
    });

    let started = Instant::now();
    let error = runtime
        .probe_with_timeout(
            AgentProbeRequest {
                agent_id: "codex".to_string(),
            },
            Duration::from_millis(200),
        )
        .unwrap_err();
    let error_text = error.to_string();

    assert!(matches!(error, RuntimeError::NotReady(_)));
    assert!(
        error_text.contains("ACP Agent probe timed out"),
        "{error_text}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let pid = fs::read_to_string(&pid_file).expect("agent pid file");
    for _ in 0..20 {
        if !process_exists(pid.trim()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !process_exists(pid.trim()),
        "hanging probe process stayed alive"
    );
}

#[test]
fn normalized_session_cwd_uses_absolute_fallback_for_no_workspace() {
    assert!(normalized_session_cwd("").is_absolute());
}

#[test]
fn probe_reports_missing_agent_command_as_setup_error() {
    let runtime = AcpAgentRuntime::new(AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "definitely-missing-openaide-agent".to_string(),
        args: Vec::new(),
        env: Vec::new(),
        secret_env: Vec::new(),
    });

    let error = runtime
        .probe_with_timeout(
            AgentProbeRequest {
                agent_id: "codex".to_string(),
            },
            Duration::from_millis(200),
        )
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("Agent command not found: definitely-missing-openaide-agent"),
        "{error}"
    );
    assert!(!error.contains("ACP error"), "{error}");
}

#[test]
fn close_tasks_run_in_parallel() {
    let completed = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    close_in_parallel(
        (0..3)
            .map(|_| {
                let completed = completed.clone();
                Box::new(move || {
                    thread::sleep(Duration::from_millis(100));
                    completed.fetch_add(1, Ordering::SeqCst);
                }) as Box<dyn FnOnce() + Send + 'static>
            })
            .collect(),
    );

    assert_eq!(completed.load(Ordering::SeqCst), 3);
    assert!(start.elapsed() < Duration::from_millis(250));
}

#[test]
fn tool_call_update_keeps_partial_fields_from_existing_call() {
    let capture = Arc::new(CapturingEventSink::default());
    let sink: Arc<dyn AgentEventSink> = capture.clone();
    let projection =
        LivePromptProjection::new("codex", sink, crate::agent::TurnCancellation::new());

    projection
        .emit(SessionUpdate::ToolCall(
            ToolCall::new("tool_call_1", "Read configuration")
                .kind(ToolKind::Read)
                .status(ToolCallStatus::InProgress),
        ))
        .unwrap();
    projection
        .emit(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool_call_1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .content(vec![ToolCallContent::from(ContentBlock::Text(
                    TextContent::new("Found configuration"),
                ))]),
        )))
        .unwrap();

    let events = capture.events();
    assert_eq!(events.len(), 2);
    match &events[1] {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(tool_call.tool_call_id, "tool_call_1");
            assert_eq!(tool_call.title, "Read configuration");
            assert_eq!(tool_call.kind, "read");
            assert_eq!(tool_call.status, AgentToolCallStatus::Completed);
            assert_eq!(
                tool_call.output_preview.as_deref(),
                Some("Found configuration")
            );
        }
        other => panic!("expected tool call event, got {other:?}"),
    }
}

#[test]
fn running_tool_output_bursts_publish_only_presentation_changes() {
    let capture = Arc::new(CapturingEventSink::default());
    let sink: Arc<dyn AgentEventSink> = capture.clone();
    let projection =
        LivePromptProjection::new("codex", sink, crate::agent::TurnCancellation::new());

    projection
        .emit(SessionUpdate::ToolCall(
            ToolCall::new("tool_call_burst", "Run tests")
                .kind(ToolKind::Execute)
                .status(ToolCallStatus::InProgress),
        ))
        .unwrap();
    for index in 0..100 {
        projection
            .emit(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                "tool_call_burst",
                ToolCallUpdateFields::new()
                    .status(ToolCallStatus::InProgress)
                    .raw_output(serde_json::json!({ "formatted_output": format!("line {index}") })),
            )))
            .unwrap();
    }
    projection
        .emit(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool_call_burst",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )))
        .unwrap();

    let events = capture.events();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events.last(),
        Some(AgentEvent::ToolCall(tool_call))
            if tool_call.status == AgentToolCallStatus::Completed
    ));
}

#[test]
fn permission_tool_call_update_seeds_later_partial_tool_updates() {
    let capture = Arc::new(CapturingEventSink::default());
    let sink: Arc<dyn AgentEventSink> = capture.clone();
    let projection =
        LivePromptProjection::new("codex", sink, crate::agent::TurnCancellation::new());
    let request = RequestPermissionRequest::new(
        "session_1",
        ToolCallUpdate::new(
            "tool_call_perm",
            ToolCallUpdateFields::new()
                .title("Allow file write".to_string())
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::Pending),
        ),
        vec![PermissionOption::new(
            "allow-once",
            "Allow once",
            PermissionOptionKind::AllowOnce,
        )],
    );

    projection.merge_tool_call_update(request.tool_call.clone());
    projection
        .emit(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool_call_perm",
            ToolCallUpdateFields::new().status(ToolCallStatus::InProgress),
        )))
        .unwrap();

    let events = capture.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(tool_call.title, "Allow file write");
            assert_eq!(tool_call.kind, "edit");
            assert_eq!(tool_call.status, AgentToolCallStatus::InProgress);
        }
        other => panic!("expected tool call event, got {other:?}"),
    }
}

#[test]
fn minimal_permission_uses_existing_tool_call_attribution() {
    let capture = Arc::new(CapturingEventSink::default());
    let sink: Arc<dyn AgentEventSink> = capture.clone();
    let projection =
        LivePromptProjection::new("codex", sink, crate::agent::TurnCancellation::new());
    projection.remember_tool_call(
        ToolCall::new("tool_call_perm", "Allow file write")
            .kind(ToolKind::Edit)
            .status(ToolCallStatus::Pending),
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let response = runtime
        .block_on(
            projection.permission_response(RequestPermissionRequest::new(
                "session_1",
                ToolCallUpdate::new("tool_call_perm", ToolCallUpdateFields::new()),
                vec![PermissionOption::new(
                    "allow-once",
                    "Allow once",
                    PermissionOptionKind::AllowOnce,
                )],
            )),
        )
        .unwrap();

    assert_eq!(response.outcome, RequestPermissionOutcome::Cancelled);
    let permissions = capture.permissions();
    assert_eq!(permissions.len(), 1);
    assert_eq!(permissions[0].title, "Allow file write");
    assert_eq!(permissions[0].tool_call.title, "Allow file write");
    assert_eq!(permissions[0].tool_call.kind.as_deref(), Some("edit"));
}

#[test]
fn prompt_content_includes_text_and_resource_links_for_path_attachments() {
    let blocks = build_prompt_content_with_policy(
        "Use attached context".to_string(),
        vec![
            Attachment {
                kind: "file".to_string(),
                label: "main #1.rs".to_string(),
                path: Some("/workspace/src/main #1.rs".to_string()),
                payload: None,
            },
            Attachment {
                kind: "file".to_string(),
                label: "windows.rs".to_string(),
                path: Some("C:\\Users\\Ada\\file 50%.rs".to_string()),
                payload: None,
            },
            Attachment {
                kind: "file".to_string(),
                label: "encoded.rs".to_string(),
                path: Some("file:///workspace/src/already%20encoded.rs".to_string()),
                payload: None,
            },
            Attachment {
                kind: "file".to_string(),
                label: "literal-percent.rs".to_string(),
                path: Some("/workspace/src/literal 50%.rs".to_string()),
                payload: None,
            },
        ],
        PromptContentPolicy::new(PromptContentCapabilities::default()),
    )
    .unwrap();

    assert_eq!(blocks.len(), 5);
    match &blocks[0] {
        ContentBlock::Text(text) => assert_eq!(text.text, "Use attached context"),
        other => panic!("expected text block, got {other:?}"),
    }
    match &blocks[1] {
        ContentBlock::ResourceLink(resource) => {
            assert_eq!(resource.name, "main #1.rs");
            assert_eq!(resource.uri, "file:///workspace/src/main%20%231.rs");
        }
        other => panic!("expected resource link, got {other:?}"),
    }
    match &blocks[2] {
        ContentBlock::ResourceLink(resource) => {
            assert_eq!(resource.name, "windows.rs");
            assert_eq!(resource.uri, "file:///C%3A/Users/Ada/file%2050%25.rs");
        }
        other => panic!("expected resource link, got {other:?}"),
    }
    match &blocks[3] {
        ContentBlock::ResourceLink(resource) => {
            assert_eq!(resource.name, "encoded.rs");
            assert_eq!(resource.uri, "file:///workspace/src/already%20encoded.rs");
        }
        other => panic!("expected resource link, got {other:?}"),
    }
    match &blocks[4] {
        ContentBlock::ResourceLink(resource) => {
            assert_eq!(resource.name, "literal-percent.rs");
            assert_eq!(resource.uri, "file:///workspace/src/literal%2050%25.rs");
        }
        other => panic!("expected resource link, got {other:?}"),
    }
}

#[test]
fn tool_call_preview_does_not_expose_raw_fields_or_full_diff_paths() {
    let raw = tool_call_event(
        &ToolCall::new("tool_call_raw", "Raw output")
            .kind(ToolKind::Execute)
            .raw_input(serde_json::json!({"path":"/secret/input"}))
            .raw_output(serde_json::json!({"token":"secret"})),
    );
    match raw {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(tool_call.input_summary.as_deref(), Some("input"));
            assert_eq!(tool_call.output_preview, None);
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    let command = tool_call_event(
        &ToolCall::new("tool_call_command", "Shell command")
            .kind(ToolKind::Execute)
            .raw_input(serde_json::json!({
                "cmd":"printf token=secret /workspace/project/.env"
            })),
    );
    match command {
        AgentEvent::ToolCall(tool_call) => {
            let summary = tool_call.input_summary.unwrap();
            assert!(summary.contains("printf"));
            assert!(summary.contains("token=[redacted]"));
            assert!(summary.contains(".env"));
            assert!(!summary.contains("secret"));
            assert!(!summary.contains("/workspace/project"));
            let details = tool_call.details.expect("command details");
            let input = details.input.expect("command input");
            assert_eq!(
                input
                    .fields
                    .iter()
                    .find(|field| field.name == "cmd")
                    .map(|field| field.value.as_str()),
                Some("printf token=[redacted] .env")
            );
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    let command_array = tool_call_event(
        &ToolCall::new("tool_call_command_array", "Search files")
            .kind(ToolKind::Search)
            .raw_input(serde_json::json!({
                "command":["zsh", "-lc", "find . -name 'index.md' -print"],
                "cwd":"/workspace/sample-project"
            })),
    );
    match command_array {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(
                tool_call.input_summary.as_deref(),
                Some("find . -name 'index.md' -print")
            );
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    let long_text = "a".repeat(181);
    let long_output = tool_call_event(
        &ToolCall::new("tool_call_long_output", "Long output").content(vec![
            ToolCallContent::from(ContentBlock::Text(TextContent::new(long_text))),
        ]),
    );
    match long_output {
        AgentEvent::ToolCall(tool_call) => {
            let expected_preview = format!("{}...", "a".repeat(180));
            assert_eq!(
                tool_call.output_preview.as_deref(),
                Some(expected_preview.as_str())
            );
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    let diff = tool_call_event(&ToolCall::new("tool_call_diff", "Edit file").content(vec![
        ToolCallContent::from(Diff::new("/secret/path/config.toml", "new")),
    ]));
    match diff {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(tool_call.output_preview.as_deref(), Some("Changed file"));
            let details = tool_call.details.expect("diff details");
            match &details.content[0] {
                ActivityToolContent::Diff { path, new_text, .. } => {
                    assert_eq!(path, "/secret/path/config.toml");
                    assert_eq!(new_text, "new");
                }
                other => panic!("expected diff content, got {other:?}"),
            }
        }
        other => panic!("expected tool call event, got {other:?}"),
    }
}

#[test]
fn tool_call_event_identifies_skill_instruction_reads() {
    let skill = tool_call_event(
        &ToolCall::new("tool_call_skill", "Read file")
            .kind(ToolKind::Read)
            .locations(vec![ToolCallLocation::new(
                "/home/user/.agents/skills/tdd/SKILL.md",
            )]),
    );
    match skill {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(tool_call.kind, "skill");
            assert_eq!(tool_call.input_summary.as_deref(), Some("tdd"));
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    let ordinary = tool_call_event(
        &ToolCall::new("tool_call_ordinary_skill_file", "Read file")
            .kind(ToolKind::Read)
            .locations(vec![ToolCallLocation::new("/workspace/SKILL.md")]),
    );
    match ordinary {
        AgentEvent::ToolCall(tool_call) => assert_eq!(tool_call.kind, "read"),
        other => panic!("expected tool call event, got {other:?}"),
    }
}

#[test]
fn tool_call_event_identifies_web_search_rows() {
    let event = tool_call_event(
        &ToolCall::new("tool_call_web_search", "Web search")
            .kind(ToolKind::Search)
            .raw_input(serde_json::json!({
                "id": "ws_internal_id",
                "type": "webSearch",
                "query": "Saint Petersburg weather tomorrow ...",
                "action": {
                    "type": "search",
                    "queries": [
                        "Saint Petersburg weather tomorrow",
                        "Санкт-Петербург погода завтра"
                    ]
                }
            })),
    );

    match event {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(tool_call.kind, "web_search");
            assert_eq!(
                tool_call.input_summary.as_deref(),
                Some("Saint Petersburg weather tomorrow ...")
            );
            let details = serde_json::to_value(tool_call.details.expect("web search details"))
                .expect("serialize web search details");
            assert_eq!(
                details.pointer("/input/queries"),
                Some(&serde_json::json!([
                    "Saint Petersburg weather tomorrow",
                    "Санкт-Петербург погода завтра"
                ]))
            );
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    let id_only_event = tool_call_event(
        &ToolCall::new("tool_call_web_search_update", "Web search")
            .kind(ToolKind::Search)
            .raw_input(serde_json::json!({
                "id": "ws_internal_id",
                "type": "webSearch"
            })),
    );
    match id_only_event {
        AgentEvent::ToolCall(tool_call) => {
            assert_eq!(tool_call.kind, "web_search");
            assert_eq!(tool_call.input_summary, None);
        }
        other => panic!("expected tool call event, got {other:?}"),
    }
}

fn config_catalog(current_value: &str) -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "mode".to_string(),
            label: "Mode".to_string(),
            description: None,
            category: Some(ConfigOptionCategory::Mode),
            current_value: current_value.to_string(),
            values: vec![
                ConfigOptionValue {
                    id: "ask".to_string(),
                    label: "Ask".to_string(),
                    description: None,
                    group_id: None,
                    group_label: None,
                },
                ConfigOptionValue {
                    id: "code".to_string(),
                    label: "Code".to_string(),
                    description: None,
                    group_id: None,
                    group_label: None,
                },
            ],
        }],
    }
}

fn command_catalog(command_name: &str) -> AgentCommandsCatalog {
    crate::agent::acp_commands_projection::normalize_available_commands(
        AvailableCommandsUpdate::new(vec![AvailableCommand::new(command_name, "Search the web")
            .input(AvailableCommandInput::Unstructured(
                UnstructuredCommandInput::new("query"),
            ))]),
    )
}

#[cfg(unix)]
fn process_exists(pid: &str) -> bool {
    Path::new("/proc").join(pid).exists()
}
