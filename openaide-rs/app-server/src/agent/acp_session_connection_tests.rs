use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::agent::acp_schema::{
    ContentBlock, ContentChunk, CreateTerminalRequest, KillTerminalRequest, LoadSessionRequest,
    LoadSessionResponse, NewSessionRequest, NewSessionResponse, PermissionOption,
    PermissionOptionKind, ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest,
    RequestPermissionOutcome, RequestPermissionRequest, SessionUpdate, TerminalOutputRequest,
    TextContent, ToolCallUpdate, ToolCallUpdateFields, WaitForTerminalExitRequest,
    WriteTextFileRequest,
};
use agent_client_protocol::Client;

use crate::agent::acp_elicitation_wire::ElicitationCreateResponse;
use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwnerId};
use crate::agent::acp_session_lifecycle::LoadReplayCapture;
use crate::agent::acp_trace::AcpTraceState;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::events::{AgentEvent, AgentPermissionOutcome, AgentPermissionRequest};
use crate::agent::{AgentEventSink, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

#[derive(Clone)]
struct AllHostHandlersConnectionTestAgent {
    done_tx: mpsc::Sender<()>,
}

struct LoadReplayConnectionTestAgent;

#[derive(Clone)]
struct ElicitationConnectionTestAgent {
    done_tx: mpsc::Sender<()>,
}

#[derive(Clone)]
struct PermissionThenUpdateConnectionTestAgent {
    permission_finished: Arc<AtomicBool>,
}

struct DelayedPermissionSink {
    requested_tx: mpsc::Sender<()>,
    release_rx: Mutex<mpsc::Receiver<()>>,
}

struct CancellingQuestionSink;

impl crate::agent::AgentSessionEventSink for CancellingQuestionSink {
    fn config_options_changed(
        &self,
        _catalog: crate::protocol::model::ConfigOptionsCatalog,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn commands_changed(
        &self,
        _catalog: crate::protocol::model::AgentCommandsCatalog,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}

impl AgentEventSink for DelayedPermissionSink {
    fn emit(&self, _event: AgentEvent) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn request_permission(
        &self,
        _request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        let _ = self.requested_tx.send(());
        self.release_rx
            .lock()
            .expect("permission release lock poisoned")
            .recv_timeout(Duration::from_secs(2))
            .expect("permission should be released");
        Ok(AgentPermissionOutcome::Cancelled)
    }
}

impl agent_client_protocol::ConnectTo<Client> for LoadReplayConnectionTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        Agent
            .builder()
            .name("load-replay-connection-test-agent")
            .on_receive_request(
                async move |request: LoadSessionRequest, responder, connection| {
                    connection.send_notification(SessionNotification::new(
                        "other_session",
                        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new("ignored"),
                        ))),
                    ))?;
                    connection.send_notification(SessionNotification::new(
                        request.session_id.clone(),
                        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new("replayed"),
                        ))),
                    ))?;
                    responder.respond(LoadSessionResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(client)
    }
}

impl agent_client_protocol::ConnectTo<Client> for AllHostHandlersConnectionTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        Agent
            .builder()
            .name("all-host-handlers-connection-test-agent")
            .connect_with(client, async move |connection| {
                let permission = connection
                    .send_request(RequestPermissionRequest::new(
                        "session_1",
                        ToolCallUpdate::new("tool_call_perm", ToolCallUpdateFields::new()),
                        vec![PermissionOption::new(
                            "allow-once",
                            "Allow once",
                            PermissionOptionKind::AllowOnce,
                        )],
                    ))
                    .block_task()
                    .await?;
                assert_eq!(permission.outcome, RequestPermissionOutcome::Cancelled);

                connection
                    .send_request(ReadTextFileRequest::new(
                        "session_1",
                        "/workspace/app/src/main.rs",
                    ))
                    .block_task()
                    .await?;
                connection
                    .send_request(WriteTextFileRequest::new(
                        "session_1",
                        "/workspace/app/src/main.rs",
                        "updated\n",
                    ))
                    .block_task()
                    .await?;
                connection
                    .send_request(CreateTerminalRequest::new("session_1", "npm"))
                    .block_task()
                    .await?;
                connection
                    .send_request(TerminalOutputRequest::new("session_1", "term_1"))
                    .block_task()
                    .await?;
                connection
                    .send_request(WaitForTerminalExitRequest::new("session_1", "term_1"))
                    .block_task()
                    .await?;
                connection
                    .send_request(KillTerminalRequest::new("session_1", "term_1"))
                    .block_task()
                    .await?;
                connection
                    .send_request(ReleaseTerminalRequest::new("session_1", "term_1"))
                    .block_task()
                    .await?;

                let _ = self.done_tx.send(());
                Ok(())
            })
    }
}

impl agent_client_protocol::ConnectTo<Client> for ElicitationConnectionTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        Agent
            .builder()
            .name("elicitation-connection-test-agent")
            .connect_with(client, async move |connection| {
                let request =
                    serde_json::from_value::<ElicitationCreateRequest>(serde_json::json!({
                        "sessionId": "session_1",
                        "toolCallId": "question_1",
                        "mode": "form",
                        "message": "Choose a direction",
                        "requestedSchema": {
                            "type": "object",
                            "properties": {
                                "direction": {
                                    "type": "string",
                                    "enum": ["left", "right"]
                                }
                            },
                            "required": ["direction"]
                        }
                    }))
                    .expect("valid elicitation request");
                let response = connection.send_request(request).block_task().await?;
                assert!(matches!(response, ElicitationCreateResponse::Cancel));
                let _ = self.done_tx.send(());
                Ok(())
            })
    }
}

impl agent_client_protocol::ConnectTo<Client> for PermissionThenUpdateConnectionTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        Agent
            .builder()
            .name("permission-then-update-connection-test-agent")
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, _connection| {
                    responder.respond(NewSessionResponse::new("opened_during_permission"))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(client, async move |connection| {
                let permission_finished = self.permission_finished.clone();
                let permission_request = connection.send_request(RequestPermissionRequest::new(
                    "permission_session",
                    ToolCallUpdate::new("tool_call_perm", ToolCallUpdateFields::new()),
                    vec![PermissionOption::new(
                        "allow-once",
                        "Allow once",
                        PermissionOptionKind::AllowOnce,
                    )],
                ));
                connection.spawn(async move {
                    permission_request.block_task().await?;
                    permission_finished.store(true, Ordering::Release);
                    Ok(())
                })?;
                connection.send_notification(SessionNotification::new(
                    "streaming_session",
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new("still streaming"),
                    ))),
                ))?;
                while !self.permission_finished.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Ok(())
            })
    }
}

async fn wait_for_done(done_rx: mpsc::Receiver<()>) -> agent_client_protocol::Result<()> {
    tokio::task::spawn_blocking(move || done_rx.recv_timeout(Duration::from_secs(1)))
        .await
        .map_err(agent_client_protocol::util::internal_error)?
        .map_err(agent_client_protocol::util::internal_error)
}

fn wait_for_replay_update(replay: &LoadReplayCaptures, session_id: &str) -> Vec<SessionUpdate> {
    let started = std::time::Instant::now();
    loop {
        let updates = replay
            .lock()
            .expect("load replay lock poisoned")
            .get(session_id)
            .expect("load replay should remain active")
            .updates
            .clone();
        if !updates.is_empty() {
            return updates;
        }
        if started.elapsed() > Duration::from_secs(1) {
            return updates;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

async fn wait_for_replay_update_async(
    replay: &LoadReplayCaptures,
    session_id: &str,
    timeout: Duration,
) -> Vec<SessionUpdate> {
    let started = std::time::Instant::now();
    loop {
        let updates = replay
            .lock()
            .expect("load replay lock poisoned")
            .get(session_id)
            .expect("load replay should remain active")
            .updates
            .clone();
        if !updates.is_empty() || started.elapsed() >= timeout {
            return updates;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn connection_context(
    host_bridge: HostBridge,
    load_replay: LoadReplayCaptures,
) -> AcpSessionConnectionContext {
    connection_context_with_trace(host_bridge, None, load_replay)
}

fn connection_context_with_trace(
    host_bridge: HostBridge,
    trace: Option<AcpTraceSession>,
    load_replay: LoadReplayCaptures,
) -> AcpSessionConnectionContext {
    let terminal_registry = AcpHostTerminalRegistry::new(host_bridge.clone());
    let owner_id = AcpTerminalOwnerId::next();
    terminal_registry.begin_open(owner_id);
    terminal_registry
        .owner(owner_id)
        .activate_session("session_1");
    AcpSessionConnectionContext {
        host_bridge,
        trace,
        current_prompts: Arc::default(),
        load_replay,
        terminal_registry,
        session_event_sinks: Arc::default(),
        session_traces: Arc::default(),
        elicitation_cancellations: Arc::default(),
    }
}

fn run_connection_in_thread(
    host_bridge: HostBridge,
    done_rx: mpsc::Receiver<()>,
    done_tx: mpsc::Sender<()>,
) -> std::thread::JoinHandle<agent_client_protocol::Result<()>> {
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            connect_acp_session_client(
                AllHostHandlersConnectionTestAgent { done_tx },
                connection_context(host_bridge, Arc::default()),
                async |_connection| wait_for_done(done_rx).await,
            )
            .await
        })
    })
}

fn expect_host_request(
    requests: &mpsc::Receiver<crate::protocol::host::HostRequest>,
    method: &str,
) -> crate::protocol::host::HostRequest {
    let request = requests
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|_| panic!("{method} request should route through host bridge"));
    assert_eq!(request.method, method);
    assert_eq!(request.params.as_ref().unwrap()["sessionId"], "session_1");
    request
}

fn wait_for_trace_file(trace_dir: &std::path::Path) -> std::path::PathBuf {
    let started = std::time::Instant::now();
    loop {
        if let Ok(mut entries) = std::fs::read_dir(trace_dir) {
            if let Some(Ok(entry)) = entries.next() {
                return entry.path();
            }
        }
        if started.elapsed() > Duration::from_secs(1) {
            panic!("trace file");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_trace_content(trace_dir: &std::path::Path) -> String {
    let trace_file = wait_for_trace_file(trace_dir);
    let started = std::time::Instant::now();
    loop {
        let content = std::fs::read_to_string(&trace_file).expect("trace content");
        if !content.is_empty() {
            return content;
        }
        if started.elapsed() > Duration::from_secs(1) {
            return content;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn connection_captures_matching_load_replay_updates() {
    let load_replay = Arc::new(Mutex::new(HashMap::from([(
        "session_1".to_string(),
        LoadReplayCapture {
            session_id: "session_1".into(),
            updates: Vec::new(),
        },
    )])));
    let replay = load_replay.clone();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        connect_acp_session_client(
            LoadReplayConnectionTestAgent,
            connection_context(HostBridge::disabled(), load_replay),
            async |connection| {
                connection
                    .send_request(LoadSessionRequest::new(
                        "session_1",
                        std::env::current_dir().unwrap_or_else(|_| "/".into()),
                    ))
                    .block_task()
                    .await?;
                Ok(())
            },
        )
        .await
        .unwrap();
    });

    let updates = wait_for_replay_update(&replay, "session_1");
    assert_eq!(updates.len(), 1);
    match &updates[0] {
        SessionUpdate::UserMessageChunk(chunk) => match &chunk.content {
            ContentBlock::Text(text) => assert_eq!(text.text, "replayed"),
            other => panic!("expected replayed text chunk, got {other:?}"),
        },
        other => panic!("expected replayed user message chunk, got {other:?}"),
    }
}

#[test]
fn pending_permission_does_not_block_updates_for_other_sessions() {
    let load_replay = Arc::new(Mutex::new(HashMap::from([(
        "streaming_session".to_string(),
        LoadReplayCapture {
            session_id: "streaming_session".into(),
            updates: Vec::new(),
        },
    )])));
    let replay = load_replay.clone();
    let (requested_tx, requested_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let permission_finished = Arc::new(AtomicBool::new(false));
    let sink = Arc::new(DelayedPermissionSink {
        requested_tx,
        release_rx: Mutex::new(release_rx),
    });
    let current_prompts = Arc::new(Mutex::new(HashMap::from([(
        "permission_session".to_string(),
        LivePromptProjection::new("codex", sink, TurnCancellation::new()),
    )])));
    let mut context = connection_context(HostBridge::disabled(), load_replay);
    context.current_prompts = current_prompts;

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        connect_acp_session_client(
            PermissionThenUpdateConnectionTestAgent {
                permission_finished,
            },
            context,
            async |_connection| {
                tokio::task::spawn_blocking(move || {
                    requested_rx
                        .recv_timeout(Duration::from_secs(1))
                        .expect("permission request should reach the host");
                    std::thread::sleep(Duration::from_millis(600));
                    let _ = release_tx.send(());
                });
                let updates = wait_for_replay_update_async(
                    &replay,
                    "streaming_session",
                    Duration::from_millis(250),
                )
                .await;
                assert_eq!(updates.len(), 1, "later update was dispatch-blocked");
                let opened = tokio::time::timeout(
                    Duration::from_millis(250),
                    _connection
                        .send_request(NewSessionRequest::new(
                            std::env::current_dir().unwrap_or_else(|_| "/".into()),
                        ))
                        .block_task(),
                )
                .await
                .expect("session/new response was dispatch-blocked")?;
                assert_eq!(opened.session_id.to_string(), "opened_during_permission");
                tokio::time::sleep(Duration::from_millis(500)).await;
                Ok(())
            },
        )
        .await
        .unwrap();
    });
}

#[test]
fn connection_registers_all_agent_to_client_host_handlers() {
    let (host_bridge, host_requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));
    let host_bridge_for_thread = host_bridge.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let handle = run_connection_in_thread(host_bridge_for_thread, done_rx, done_tx);

    let read = expect_host_request(&host_requests, "fs/read_text_file");
    assert_eq!(
        read.params.as_ref().unwrap()["path"],
        "/workspace/app/src/main.rs"
    );
    assert!(host_bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": read.id,
        "result": ReadTextFileResponse::new("from host"),
    })));

    let write = expect_host_request(&host_requests, "fs/write_text_file");
    assert_eq!(write.params.as_ref().unwrap()["content"], "updated\n");
    assert!(host_bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": write.id,
        "result": null,
    })));

    let create = expect_host_request(&host_requests, "terminal/create");
    assert_eq!(create.params.as_ref().unwrap()["command"], "npm");
    assert!(host_bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": create.id,
        "result": { "terminalId": "term_1" },
    })));

    let output = expect_host_request(&host_requests, "terminal/output");
    assert_eq!(output.params.as_ref().unwrap()["terminalId"], "term_1");
    assert!(host_bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": output.id,
        "result": { "output": "ready", "truncated": false },
    })));

    let wait = expect_host_request(&host_requests, "terminal/wait_for_exit");
    assert_eq!(wait.params.as_ref().unwrap()["terminalId"], "term_1");
    assert!(host_bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": wait.id,
        "result": { "exitCode": 0 },
    })));

    let kill = expect_host_request(&host_requests, "terminal/kill");
    assert_eq!(kill.params.as_ref().unwrap()["terminalId"], "term_1");
    assert!(host_bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": kill.id,
        "result": null,
    })));

    let release = expect_host_request(&host_requests, "terminal/release");
    assert_eq!(release.params.as_ref().unwrap()["terminalId"], "term_1");
    assert!(host_bridge.try_handle_response(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": release.id,
        "result": null,
    })));

    handle.join().expect("connection thread").unwrap();
}

#[test]
fn connection_traces_elicitation_request_and_response_to_owning_session() {
    let temp = tempfile::TempDir::new().expect("trace temp dir");
    let trace_state = AcpTraceState::disabled(temp.path());
    trace_state.set_enabled(true).expect("enable ACP trace");
    let trace = AcpTraceSession::new(trace_state, "task_1", "connection-test");
    let mut context = connection_context(HostBridge::disabled(), Arc::default());
    context.session_event_sinks = Arc::new(Mutex::new(HashMap::from([(
        "session_1".to_string(),
        Arc::new(CancellingQuestionSink) as Arc<dyn crate::agent::AgentSessionEventSink>,
    )])));
    context.session_traces = Arc::new(Mutex::new(HashMap::from([(
        "session_1".to_string(),
        trace,
    )])));
    let (done_tx, done_rx) = mpsc::channel();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        connect_acp_session_client(
            ElicitationConnectionTestAgent { done_tx },
            context,
            async |_connection| wait_for_done(done_rx).await,
        )
        .await
        .unwrap();
    });

    let trace_dir = temp.path().join("diagnostics").join("acp-traces");
    let trace_content = wait_for_trace_content(&trace_dir);
    assert!(trace_content.contains("\"event\":\"elicitation/create.request\""));
    assert!(trace_content.contains("\"event\":\"elicitation/create.response\""));
    assert!(trace_content.contains("\"sessionId\":\"session_1\""));
    assert!(trace_content.contains("\"toolCallId\":\"question_1\""));
    assert!(trace_content.contains("\"action\":\"cancel\""));
}

#[test]
fn notification_handler_traces_and_forwards_unmatched_updates_without_retry() {
    let temp = tempfile::TempDir::new().expect("trace temp dir");
    let trace_state = AcpTraceState::disabled(temp.path());
    trace_state.set_enabled(true).expect("enable ACP trace");
    let trace = AcpTraceSession::new(trace_state, "task_1", "connection-test");
    let load_replay = Arc::new(Mutex::new(HashMap::new()));
    let notification = SessionNotification::new(
        "other_session",
        SessionUpdate::UserMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "forwarded",
        )))),
    );

    let forwarded = handle_session_update_notification(notification, &Some(trace), &load_replay)
        .expect("unmatched update should be forwarded");
    match unhandled_session_update(forwarded, ()) {
        Handled::No { retry, .. } => assert!(!retry),
        Handled::Yes => panic!("unmatched update should not be handled"),
    }

    let trace_dir = temp.path().join("diagnostics").join("acp-traces");
    let trace_content = wait_for_trace_content(&trace_dir);
    assert!(trace_content.contains("\"event\":\"session/update\""));
    assert!(trace_content.contains("\"sessionId\":\"other_session\""));
}
