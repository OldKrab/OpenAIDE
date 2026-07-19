use openaide_app_server_protocol::client::{
    ClientCapabilities, ClientCapabilitiesChangedParams, ClientProtocolCapability,
    ClientWorkspaceRoot, InitializeParams, RequestedSurface, ShellCapability, ShellDescriptor,
    ShellKind,
};
use openaide_app_server_protocol::envelopes::{ErrorEnvelope, RequestMeta};
use openaide_app_server_protocol::errors::ProtocolErrorCode;
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{ClientInstanceId, ClientRequestId, StateRootId, TaskId};
use openaide_app_server_protocol::methods::{
    AGENT_AUTHENTICATE, AGENT_LIST_SESSIONS, ATTACHMENT_REVEAL, CLIENT_CAPABILITIES_CHANGED,
    CLIENT_HEARTBEAT, CLIENT_INITIALIZE, DIAGNOSTICS_GET_RUNTIME, SETTINGS_GET_MCP_SERVERS,
    SETTINGS_GET_PREFERENCES, SETTINGS_GET_RUNTIME, SETTINGS_GET_SKILLS,
    SETTINGS_UPDATE_PREFERENCES, SETTINGS_UPDATE_RUNTIME, SHELL_RESOLVE_FILE_REVEAL,
    STATE_SUBSCRIBE, STATE_UNSUBSCRIBE, TASK_CHAT_PAGE,
};
use openaide_app_server_protocol::settings::{
    AppPreferencesPatch, AppPreferencesUpdateParams, ComposerSubmitShortcut,
    RuntimeAcpTraceSettingsPatch, RuntimeDeveloperSettingsPatch, RuntimeSettingsUpdateParams,
};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use openaide_app_server_protocol::state::{
    StateSubscribeParams, StateUnsubscribeParams, SubscriptionScope,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::agent::product_api::{
    AgentAuthenticateWorkflow, AgentCatalogMutationWorkflow, AgentProbeWorkflow,
    AgentSettingsDetailsWorkflow,
};
use crate::app_lifecycle::{AppLifecycle, LifecycleState};
use crate::attachment_runtime::ResolvedRevealAttachment;
use crate::client_lifecycle::{AppServerTime, ClientExpiryOutcome, ClientHub, ConnectionId};
use crate::diagnostics::RuntimeDiagnosticsWorkflow;
use crate::server_requests::ServerRequestRuntime;
use crate::server_requests::{OpenRequestOutcome, ServerRequestAnswer, ServerRequestDraft};
use crate::settings::{
    AppPreferencesService, McpServersSettingsService, RuntimeSettingsService, SettingsCatalog,
    SkillsSettingsService,
};
use crate::shell_file_handles::ShellFileRevealRegistry;
use crate::snapshots::{
    AgentRegistrySnapshotSource, ProjectCollectionStore, SnapshotBuilder, SnapshotSources,
    TaskListSnapshot, TaskNavigationStore, TaskSnapshotSource, TaskSnapshotStore,
};
use crate::state_sync::StateStream;
use crate::storage::Store;
use crate::task_events::{
    CommittedChatChange, CommittedTaskChange, TaskUpdate, TaskUpdateKind, ToolDetailUpdate,
};
use crate::tasks::product_api::{
    AgentListSessionsWorkflow, AttachmentFileBrowserWorkflow, TaskAcquireWorkflow,
    TaskAdoptNativeSessionWorkflow, TaskArchiveWorkflow, TaskCancelWorkflow, TaskChatPageWorkflow,
    TaskFileSearchWorkflow, TaskOpenWorkflow, TaskReleaseWorkflow, TaskSendAccepted,
    TaskSendWorkflow, TaskSetConfigOptionWorkflow,
};

use super::*;

mod client_probe;

#[test]
fn rejects_product_request_before_initialize() {
    let mut gateway = gateway();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "1",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Projects,
            },
        ),
        AppServerTime(1),
    );

    let error = response_error(outcome);
    assert_eq!(error.error.code, ProtocolErrorCode::NotInitialized);
    assert_eq!(
        error.error.message,
        "client/initialize must succeed before product requests"
    );
    assert!(error.error.recoverable);
    assert_eq!(
        error.error.target.and_then(|target| target.method),
        Some(STATE_SUBSCRIBE.to_string())
    );
}

#[test]
fn initialize_records_client_and_returns_snapshot_cursor() {
    let mut gateway = gateway();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("1", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(1),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["snapshot"]["cursor"], json!("cursor-0"));
    assert_eq!(
        value["result"]["snapshot"]["client"]["clientInstanceId"],
        json!("client-1")
    );
    assert!(gateway
        .client_hub
        .context_for_connection(&ConnectionId::new("conn-1"))
        .is_some());
}

#[test]
fn client_capabilities_changed_replaces_reported_workspace_roots() {
    let mut gateway = gateway_with_project_context();
    let connection_id = ConnectionId::new("conn-1");
    let mut params = init_params("client-1");
    params.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/alpha".to_string(),
    }];
    response_value(gateway.handle_inbound(
        connection_id.clone(),
        request("1", CLIENT_INITIALIZE, params),
        AppServerTime(1),
    ));

    let changed = response_value(gateway.handle_inbound(
        connection_id,
        request(
            "2",
            CLIENT_CAPABILITIES_CHANGED,
            ClientCapabilitiesChangedParams {
                capabilities: None,
                workspace_roots: Some(vec![ClientWorkspaceRoot {
                    path: "/workspace/beta".to_string(),
                }]),
            },
        ),
        AppServerTime(2),
    ));

    let projects = changed["result"]["projects"]["projects"]
        .as_array()
        .expect("Project collection");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["label"], json!("beta"));
}

#[test]
fn workspace_root_replacement_preserves_other_clients_projects() {
    let mut gateway = gateway_with_project_context();
    let first_connection = ConnectionId::new("conn-1");
    let second_connection = ConnectionId::new("conn-2");
    let mut first = init_params("client-1");
    first.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/alpha".to_string(),
    }];
    response_value(gateway.handle_inbound(
        first_connection.clone(),
        request("1", CLIENT_INITIALIZE, first),
        AppServerTime(1),
    ));
    let mut second = init_params("client-2");
    second.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/beta".to_string(),
    }];
    response_value(gateway.handle_inbound(
        second_connection,
        request("2", CLIENT_INITIALIZE, second),
        AppServerTime(2),
    ));

    let changed = response_value(gateway.handle_inbound(
        first_connection,
        request(
            "3",
            CLIENT_CAPABILITIES_CHANGED,
            ClientCapabilitiesChangedParams {
                capabilities: None,
                workspace_roots: Some(vec![ClientWorkspaceRoot {
                    path: "/workspace/gamma".to_string(),
                }]),
            },
        ),
        AppServerTime(3),
    ));

    let labels = changed["result"]["projects"]["projects"]
        .as_array()
        .expect("Project collection")
        .iter()
        .map(|project| project["label"].as_str().expect("Project label"))
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["beta", "gamma"]);
}

#[test]
fn expired_client_workspace_roots_leave_the_project_collection() {
    let mut gateway = gateway_with_project_context();
    let first_connection = ConnectionId::new("conn-1");
    let second_connection = ConnectionId::new("conn-2");
    let mut first = init_params("client-1");
    first.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/alpha".to_string(),
    }];
    response_value(gateway.handle_inbound(
        first_connection.clone(),
        request("1", CLIENT_INITIALIZE, first),
        AppServerTime(1),
    ));
    let mut second = init_params("client-2");
    second.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/beta".to_string(),
    }];
    response_value(gateway.handle_inbound(
        second_connection.clone(),
        request("2", CLIENT_INITIALIZE, second),
        AppServerTime(2),
    ));
    gateway.handle_transport_closed(&first_connection, AppServerTime(3));

    assert!(matches!(
        gateway.expire_client_after_reconnect_grace(
            &ClientInstanceId::from("client-1"),
            AppServerTime(13),
        ),
        ClientExpiryOutcome::Expired { .. }
    ));
    let current = response_value(gateway.handle_inbound(
        second_connection,
        request(
            "3",
            CLIENT_CAPABILITIES_CHANGED,
            ClientCapabilitiesChangedParams::default(),
        ),
        AppServerTime(14),
    ));

    let projects = current["result"]["projects"]["projects"]
        .as_array()
        .expect("Project collection");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["label"], json!("beta"));
}

#[test]
fn expired_client_workspace_roots_publish_projects_to_existing_subscribers() {
    let mut gateway = gateway_with_project_context();
    let host_connection = ConnectionId::new("conn-host");
    let webview_connection = ConnectionId::new("conn-webview");
    let mut host = init_params("client-host");
    host.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/alpha".to_string(),
    }];
    response_value(gateway.handle_inbound(
        host_connection.clone(),
        request("1", CLIENT_INITIALIZE, host),
        AppServerTime(1),
    ));
    response_value(gateway.handle_inbound(
        webview_connection.clone(),
        request("2", CLIENT_INITIALIZE, init_params("client-webview")),
        AppServerTime(2),
    ));
    response_value(gateway.handle_inbound(
        webview_connection.clone(),
        request(
            "3",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Projects,
            },
        ),
        AppServerTime(3),
    ));
    gateway.handle_transport_closed(&host_connection, AppServerTime(4));

    assert!(matches!(
        gateway.expire_client_after_reconnect_grace(
            &ClientInstanceId::from("client-host"),
            AppServerTime(14),
        ),
        ClientExpiryOutcome::Expired { .. }
    ));
    let events = response_events(gateway.handle_inbound(
        webview_connection,
        request("4", CLIENT_HEARTBEAT, serde_json::json!({})),
        AppServerTime(15),
    ));

    assert!(events.iter().any(|delivery| {
        delivery.delivery.client_instance_id == ClientInstanceId::from("client-webview")
            && matches!(
                &delivery.event.payload,
                AppServerEventPayload::ProjectCollectionUpdated { projects }
                    if projects.projects.is_empty()
            )
    }));
}

#[test]
fn workspace_root_changes_publish_projects_to_other_subscribed_clients() {
    let mut gateway = gateway_with_project_context();
    let host_connection = ConnectionId::new("conn-host");
    let webview_connection = ConnectionId::new("conn-webview");
    let mut host = init_params("client-host");
    host.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/alpha".to_string(),
    }];
    response_value(gateway.handle_inbound(
        host_connection.clone(),
        request("1", CLIENT_INITIALIZE, host),
        AppServerTime(1),
    ));
    response_value(gateway.handle_inbound(
        webview_connection.clone(),
        request("2", CLIENT_INITIALIZE, init_params("client-webview")),
        AppServerTime(2),
    ));
    response_value(gateway.handle_inbound(
        webview_connection,
        request(
            "3",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Projects,
            },
        ),
        AppServerTime(3),
    ));

    let events = response_events(gateway.handle_inbound(
        host_connection,
        request(
            "4",
            CLIENT_CAPABILITIES_CHANGED,
            ClientCapabilitiesChangedParams {
                capabilities: None,
                workspace_roots: Some(vec![ClientWorkspaceRoot {
                    path: "/workspace/beta".to_string(),
                }]),
            },
        ),
        AppServerTime(4),
    ));

    let project_update = events
        .into_iter()
        .find(|delivery| {
            delivery.delivery.client_instance_id == ClientInstanceId::from("client-webview")
                && matches!(
                    delivery.event.payload,
                    AppServerEventPayload::ProjectCollectionUpdated { .. }
                )
        })
        .expect("subscribed webview receives Project collection update");
    let AppServerEventPayload::ProjectCollectionUpdated { projects } = project_update.event.payload
    else {
        unreachable!("filtered to Project collection update")
    };
    assert_eq!(projects.projects.len(), 1);
    assert_eq!(projects.projects[0].label, "beta");
}

#[test]
fn initialize_workspace_roots_publish_projects_to_existing_subscribers() {
    let mut gateway = gateway_with_project_context();
    let webview_connection = ConnectionId::new("conn-webview");
    response_value(gateway.handle_inbound(
        webview_connection.clone(),
        request("1", CLIENT_INITIALIZE, init_params("client-webview")),
        AppServerTime(1),
    ));
    response_value(gateway.handle_inbound(
        webview_connection,
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Projects,
            },
        ),
        AppServerTime(2),
    ));
    let mut host = init_params("client-host");
    host.workspace_roots = vec![ClientWorkspaceRoot {
        path: "/workspace/alpha".to_string(),
    }];

    let events = response_events(gateway.handle_inbound(
        ConnectionId::new("conn-host"),
        request("3", CLIENT_INITIALIZE, host),
        AppServerTime(3),
    ));

    assert!(events.iter().any(|delivery| {
        delivery.delivery.client_instance_id == ClientInstanceId::from("client-webview")
            && matches!(
                &delivery.event.payload,
                AppServerEventPayload::ProjectCollectionUpdated { projects }
                    if projects.projects.iter().any(|project| project.label == "alpha")
            )
    }));
}

#[test]
fn agent_list_sessions_returns_typed_result_without_workspace_paths() {
    let mut gateway = gateway_with_agent_session_listing(Arc::new(ListingAgentSessions));
    let connection_id = ConnectionId::new("conn-1");
    initialize(&mut gateway, connection_id.clone());

    let outcome = gateway.handle_inbound(
        connection_id,
        request(
            "2",
            AGENT_LIST_SESSIONS,
            serde_json::json!({
                "agentId": "codex",
                "projectId": "project-1",
                "cursor": "cursor-1",
            }),
        ),
        AppServerTime(2),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["agentId"], json!("codex"));
    assert_eq!(value["result"]["projectId"], json!("project-1"));
    assert_eq!(value["result"]["projectLabel"], json!("Workspace"));
    assert_eq!(value["result"]["nextCursor"], json!("cursor-2"));
    assert_eq!(
        value["result"]["sessions"][0]["sessionId"],
        json!("session-1")
    );
    assert!(value["result"]["sessions"][0].get("cwd").is_none());
}

#[test]
fn background_native_catalog_refresh_request_is_delegated() {
    let workflow = Arc::new(RecordingCatalogRefresh::default());
    let gateway = SharedRpcGateway::new(gateway_with_agent_session_listing(workflow.clone()));

    gateway.request_native_session_catalog_refresh();

    assert_eq!(workflow.requests.load(Ordering::SeqCst), 1);
}

#[test]
fn agent_authenticate_returns_typed_result() {
    let mut gateway = gateway_with_agent_authenticate(Arc::new(AuthenticatingAgent));
    let connection_id = ConnectionId::new("conn-1");
    initialize(&mut gateway, connection_id.clone());

    let outcome = gateway.handle_inbound(
        connection_id,
        request(
            "2",
            AGENT_AUTHENTICATE,
            serde_json::json!({
                "agentId": "codex",
                "methodId": "codex-login",
            }),
        ),
        AppServerTime(2),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["agentId"], json!("codex"));
    assert_eq!(value["result"]["methodId"], json!("codex-login"));
    assert_eq!(value["result"]["status"], json!("authenticated"));
}

#[test]
fn runtime_diagnostics_returns_typed_app_server_result() {
    let mut gateway = gateway();
    let connection_id = ConnectionId::new("conn-1");
    initialize(&mut gateway, connection_id.clone());

    let outcome = gateway.handle_inbound(
        connection_id,
        request("2", DIAGNOSTICS_GET_RUNTIME, serde_json::json!({})),
        AppServerTime(2),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["status"], json!("ready"));
    assert_eq!(
        value["result"]["methodCount"],
        json!(openaide_app_server_protocol::methods::CLIENT_METHODS.len())
    );
    assert_eq!(value["result"]["tasks"]["visibleCount"], json!(2));
    assert_eq!(
        value["result"]["redaction"],
        json!("prompt_text_file_contents_terminal_output_and_secrets_removed")
    );
}

#[test]
fn task_chat_page_returns_protocol_chat_items() {
    let mut gateway = gateway();
    let connection_id = ConnectionId::new("conn-1");
    initialize(&mut gateway, connection_id.clone());

    let outcome = gateway.handle_inbound(
        connection_id,
        request(
            "2",
            TASK_CHAT_PAGE,
            serde_json::json!({
                "taskId": "task-1",
                "beforeCursor": "msg-2",
                "limit": 25,
            }),
        ),
        AppServerTime(2),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["taskId"], json!("task-1"));
    assert_eq!(value["result"]["items"][0]["messageId"], json!("msg-1"));
    assert_eq!(
        value["result"]["items"][0]["parts"][0]["text"],
        json!("older")
    );
    assert_eq!(value["result"]["hasBefore"], json!(false));
}

#[test]
fn runtime_settings_get_and_update_use_app_server_protocol() {
    let mut gateway = gateway();
    let connection_id = ConnectionId::new("conn-1");
    initialize(&mut gateway, connection_id.clone());

    let initial = gateway.handle_inbound(
        connection_id.clone(),
        request("2", SETTINGS_GET_RUNTIME, serde_json::json!({})),
        AppServerTime(2),
    );
    assert_eq!(
        response_value(initial)["result"]["developer"]["acpTrace"]["enabled"],
        json!(false)
    );

    let updated = gateway.handle_inbound(
        connection_id,
        request(
            "3",
            SETTINGS_UPDATE_RUNTIME,
            RuntimeSettingsUpdateParams {
                developer: RuntimeDeveloperSettingsPatch {
                    acp_trace: RuntimeAcpTraceSettingsPatch {
                        enabled: Some(true),
                    },
                },
            },
        ),
        AppServerTime(3),
    );

    let value = response_value(updated);
    assert_eq!(
        value["result"]["developer"]["acpTrace"]["enabled"],
        json!(true)
    );
    assert!(value["result"]["developer"]["acpTrace"]["directory"]
        .as_str()
        .unwrap()
        .ends_with("diagnostics/acp-traces"));
}

#[test]
fn non_agent_settings_reads_report_missing_discovery_sources_as_unavailable() {
    let mut gateway = gateway();
    let connection_id = ConnectionId::new("conn-1");
    initialize(&mut gateway, connection_id.clone());

    let mcp = gateway.handle_inbound(
        connection_id.clone(),
        request("2", SETTINGS_GET_MCP_SERVERS, serde_json::json!({})),
        AppServerTime(2),
    );
    let mcp = response_value(mcp);
    assert!(mcp["result"]["generatedAt"].as_str().is_some());
    assert_eq!(mcp["result"]["availability"], json!("unavailable"));
    assert_eq!(mcp["result"]["servers"], json!([]));
    assert!(mcp["result"].get("notices").is_none());

    let skills = gateway.handle_inbound(
        connection_id,
        request("3", SETTINGS_GET_SKILLS, serde_json::json!({})),
        AppServerTime(3),
    );
    let skills = response_value(skills);
    assert!(skills["result"]["generatedAt"].as_str().is_some());
    assert_eq!(skills["result"]["availability"], json!("unavailable"));
    assert_eq!(skills["result"]["skills"], json!([]));
    assert!(skills["result"].get("notices").is_none());
}

#[test]
fn app_preferences_get_and_update_use_app_server_protocol() {
    let mut gateway = gateway();
    let connection_id = ConnectionId::new("conn-1");
    initialize(&mut gateway, connection_id.clone());

    let initial = gateway.handle_inbound(
        connection_id.clone(),
        request("2", SETTINGS_GET_PREFERENCES, serde_json::json!({})),
        AppServerTime(2),
    );
    assert_eq!(
        response_value(initial)["result"]["preferences"]["composerSubmitShortcut"],
        json!("enter")
    );

    let updated = gateway.handle_inbound(
        connection_id,
        request(
            "3",
            SETTINGS_UPDATE_PREFERENCES,
            AppPreferencesUpdateParams {
                preferences: AppPreferencesPatch {
                    composer_submit_shortcut: ComposerSubmitShortcut::ModEnter,
                },
            },
        ),
        AppServerTime(3),
    );

    assert_eq!(
        response_value(updated)["result"]["preferences"]["composerSubmitShortcut"],
        json!("modEnter")
    );
}

#[test]
fn response_envelope_preserves_client_request_meta() {
    let mut gateway = gateway();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request_with_meta(
            "1",
            CLIENT_INITIALIZE,
            init_params("client-1"),
            RequestMeta {
                client_request_id: Some(ClientRequestId::from("client-request-1")),
            },
        ),
        AppServerTime(1),
    );

    let value = response_value(outcome);
    assert_eq!(value["meta"]["clientRequestId"], json!("client-request-1"));
}

#[test]
fn error_envelope_preserves_client_request_meta_and_invalid_params_details() {
    let mut gateway = gateway();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        InboundProtocolMessage::ClientRequest {
            id: "1".to_string(),
            method: CLIENT_INITIALIZE.to_string(),
            params: json!({ "clientInstanceId": 7 }),
            meta: RequestMeta {
                client_request_id: Some(ClientRequestId::from("client-request-2")),
            },
        },
        AppServerTime(1),
    );

    let error = response_error(outcome);
    assert_eq!(
        error.meta.client_request_id,
        Some(ClientRequestId::from("client-request-2"))
    );
    assert_eq!(error.error.code, ProtocolErrorCode::InvalidRequest);
    assert!(!error.error.recoverable);
    assert_eq!(
        error.error.target.and_then(|target| target.field),
        Some("params".to_string())
    );
    assert!(error.error.message.starts_with("Invalid params:"));
}

#[test]
fn initialize_after_event_uses_state_stream_cursor() {
    let mut gateway = gateway();
    gateway.state_stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationChanged {
            change: openaide_app_server_protocol::events::TaskNavigationChange::Remove {
                task_id: TaskId::from("absent"),
            },
        },
        |_| None,
        AppServerTime(1),
    );

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("1", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(2),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["snapshot"]["cursor"], json!("cursor-1"));
}

#[test]
fn initialize_during_stopping_returns_server_stopping() {
    let mut gateway = gateway();
    gateway.lifecycle.begin_stopping();

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("1", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(1),
    );

    let error = response_error(outcome);
    assert_eq!(error.error.code, ProtocolErrorCode::ServerStopping);
}

#[test]
fn subscribe_after_initialize_returns_snapshot_and_stores_subscription() {
    let mut gateway = initialized_gateway("client-1", "conn-1");

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation { project_id: None },
            },
        ),
        AppServerTime(2),
    );

    let value = response_value(outcome);
    assert_eq!(value["result"]["cursor"], json!("cursor-0"));
    assert_eq!(value["result"]["snapshot"]["kind"], json!("taskNavigation"));
    assert_eq!(gateway.state_stream.subscription_count(), 1);
}

#[test]
fn tool_detail_subscription_receives_only_full_updates_for_its_artifact() {
    use openaide_app_server_protocol::ids::MessageId;
    use openaide_app_server_protocol::snapshot::{ChatItem, ChatItemStatus, ChatRole};

    let mut gateway = initialized_gateway("client-1", "conn-1");
    let task_snapshots = Arc::new(EmptyTaskSnapshots);
    gateway.snapshots = SnapshotBuilder::with_task_snapshots(
        "server-1".into(),
        "root-1".into(),
        task_snapshots.clone(),
    );
    gateway.task_snapshots = task_snapshots;
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request("2", CLIENT_INITIALIZE, init_params("client-2")),
        AppServerTime(2),
    ));
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "3",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::ToolDetail {
                    task_id: TaskId::from("task-1"),
                    artifact_id: "artifact-2".to_string(),
                },
            },
        ),
        AppServerTime(3),
    ));
    let baseline = response_value(gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request(
            "4",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::ToolDetail {
                    task_id: TaskId::from("task-1"),
                    artifact_id: "artifact-1".to_string(),
                },
            },
        ),
        AppServerTime(4),
    ));
    assert_eq!(
        baseline["result"]["snapshot"]["details"]["content"][0]["text"],
        json!("details")
    );

    let update = committed_task_update(
        "task-1",
        2,
        vec![CommittedChatChange::Upsert {
            item: ChatItem {
                message_id: MessageId::from("tool-1"),
                turn_id: None,
                role: ChatRole::System,
                status: ChatItemStatus::Complete,
                parts: Vec::new(),
            },
        }],
        vec![ToolDetailUpdate {
            artifact_id: "artifact-1".to_string(),
            details: fixed_tool_detail(),
        }],
        TestNavigationChange::None,
    );
    let deliveries = gateway.publish_task_update(&update, AppServerTime(5));

    assert_eq!(deliveries.len(), 1);
    assert_eq!(
        deliveries[0].delivery.client_instance_id.as_str(),
        "client-2"
    );
    assert!(matches!(
        deliveries[0].event.payload,
        AppServerEventPayload::ToolDetailUpdated { .. }
    ));
}

#[test]
fn task_subscription_delivers_pending_server_request() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    let opened = gateway.open_server_request(task_secret_request("task-1"), AppServerTime(2));
    assert!(matches!(
        opened,
        OpenRequestOutcome::Opened {
            deliveries,
            ..
        } if deliveries.is_empty()
    ));

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "3",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(3),
    );

    let (value, server_requests) = response_value_and_server_requests(outcome);
    assert_eq!(
        value["result"]["snapshot"]["task"]["pendingRequests"][0]["requestId"],
        "server-request-1"
    );
    assert_eq!(server_requests.len(), 1);
    assert_eq!(server_requests[0].envelope.method, "secret/read");
}

#[test]
fn task_request_waits_when_subscribed_client_lacks_response_capability() {
    let mut gateway = gateway();
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "1",
            CLIENT_INITIALIZE,
            init_params_without_request_responses("client-1"),
        ),
        AppServerTime(1),
    );
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(2),
    );

    assert!(matches!(
        gateway.open_server_request(task_server_request("task-1"), AppServerTime(3)),
        OpenRequestOutcome::Opened { deliveries, .. } if deliveries.is_empty()
    ));
}

#[test]
fn task_request_opened_after_subscription_is_delivered_immediately() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(2),
    );

    let opened = gateway
        .server_requests
        .open_task_secret_read_request(
            TaskId::from("task-1"),
            "agent.secret".to_string(),
            Some("Agent secret".to_string()),
            AppServerTime(3),
        )
        .expect("open task secret request");
    assert_eq!(opened.deliveries.len(), 1);
    assert_eq!(opened.deliveries[0].envelope.request_id, opened.request_id);
    assert_eq!(opened.deliveries[0].envelope.method, "secret/read");

    let server_requests = gateway
        .drain_server_requests_for_connection(&ConnectionId::new("conn-1"), AppServerTime(4));

    assert!(server_requests.is_empty());
}

#[test]
fn task_permission_routes_to_all_connected_capable_clients_without_subscription_authority() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request("2", CLIENT_INITIALIZE, init_params("client-2")),
        AppServerTime(2),
    );
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "3",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(3),
    );

    let opened = gateway.open_server_request(task_server_request("task-1"), AppServerTime(4));

    let OpenRequestOutcome::Opened { deliveries, .. } = opened else {
        panic!("connected capable clients must make the permission answerable");
    };
    let mut client_ids = deliveries
        .iter()
        .map(|delivery| delivery.delivery.client_instance_id.as_str())
        .collect::<Vec<_>>();
    client_ids.sort_unstable();
    assert_eq!(client_ids, vec!["client-1", "client-2"]);
}

#[test]
fn opening_task_permission_publishes_pending_request_in_task_state_stream() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(2),
    );
    gateway
        .server_requests
        .open(task_server_request("task-1"), Vec::new(), AppServerTime(3));

    let deliveries = gateway.publish_task_update(
        &committed_task_update(
            "task-1",
            2,
            Vec::new(),
            Vec::new(),
            TestNavigationChange::None,
        ),
        AppServerTime(4),
    );

    assert!(deliveries.iter().any(|delivery| matches!(
        &delivery.event.payload,
        AppServerEventPayload::TaskRequestsUpdated { requests, .. }
            if requests.iter().any(|request| request.request_id.as_str() == "server-request-1")
    )));
}

#[test]
fn current_task_subscriber_can_answer_before_server_request_delivery_drains() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(2),
    );
    gateway
        .server_requests
        .open(task_server_request("task-1"), Vec::new(), AppServerTime(3));

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        InboundProtocolMessage::ClientResponse {
            request_id: "server-request-1".to_string(),
            answer: ServerRequestAnswer::Result(json!({ "decision": "allow" })),
        },
        AppServerTime(4),
    );

    let events = response_events(outcome);
    assert!(events.iter().any(|delivery| matches!(
        delivery.event.payload,
        AppServerEventPayload::TaskRequestsUpdated { ref requests, .. } if requests.is_empty()
    )));
    assert!(gateway
        .server_requests
        .pending_for_task(&TaskId::from("task-1"))
        .is_empty());
}

#[test]
fn client_response_resolves_pending_server_request() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(2),
    );
    let opened = gateway.open_server_request(task_server_request("task-1"), AppServerTime(3));
    assert!(matches!(
        opened,
        OpenRequestOutcome::Opened {
            deliveries,
            ..
        } if deliveries.len() == 1
    ));

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        InboundProtocolMessage::ClientResponse {
            request_id: "server-request-1".to_string(),
            answer: ServerRequestAnswer::Result(json!({ "decision": "allow" })),
        },
        AppServerTime(4),
    );

    let events = response_events(outcome);
    assert!(events.iter().any(|delivery| matches!(
        delivery.event.payload,
        AppServerEventPayload::TaskRequestsUpdated { ref requests, .. } if requests.is_empty()
    )));
    assert!(gateway
        .server_requests
        .pending_for_task(&TaskId::from("task-1"))
        .is_empty());
}

#[test]
fn unknown_client_response_returns_permission_error() {
    let mut gateway = initialized_gateway("client-1", "conn-1");

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        InboundProtocolMessage::ClientResponse {
            request_id: "server-request-1".to_string(),
            answer: ServerRequestAnswer::Result(json!({ "optionId": "allow-once" })),
        },
        AppServerTime(2),
    );

    let error = response_error(outcome);
    assert_eq!(error.error.code, ProtocolErrorCode::RequestAlreadyResolved);
    assert_eq!(
        error.error.message,
        "Permission request is no longer answerable."
    );
}

#[test]
fn heartbeat_delivers_a_queued_navigation_change_once() {
    let mut gateway = initialized_gateway("client-1", "local-http:client-1");
    gateway.handle_inbound(
        ConnectionId::new("local-http:client-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation { project_id: None },
            },
        ),
        AppServerTime(2),
    );

    let published = gateway.publish_task_update(
        &committed_task_update(
            "task-1",
            2,
            Vec::new(),
            Vec::new(),
            TestNavigationChange::Upsert,
        ),
        AppServerTime(3),
    );

    assert_eq!(published.len(), 1);

    let outcome = gateway.handle_inbound(
        ConnectionId::new("local-http:client-1"),
        request("3", CLIENT_HEARTBEAT, serde_json::json!({})),
        AppServerTime(4),
    );

    let events = response_events(outcome);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0].event.payload,
        AppServerEventPayload::TaskNavigationChanged { .. }
    ));

    let second = gateway.handle_inbound(
        ConnectionId::new("local-http:client-1"),
        request("4", CLIENT_HEARTBEAT, serde_json::json!({})),
        AppServerTime(5),
    );
    assert!(response_events(second).is_empty());
}

#[test]
fn new_task_update_is_delivered_only_to_its_owner_task_subscription() {
    let (mut gateway, store) = gateway_with_project_context_and_store();
    store
        .write_task(&client_new_task_record("task-new", "client-1"))
        .unwrap();
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("1", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(1),
    );
    gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request("2", CLIENT_INITIALIZE, init_params("client-2")),
        AppServerTime(2),
    );
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "3",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-new"),
                },
            },
        ),
        AppServerTime(3),
    ));
    response_value(gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request(
            "4",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation { project_id: None },
            },
        ),
        AppServerTime(4),
    ));

    let events = gateway.publish_task_update(
        &committed_task_update(
            "task-new",
            1,
            vec![CommittedChatChange::Replace],
            Vec::new(),
            TestNavigationChange::None,
        ),
        AppServerTime(5),
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].delivery.client_instance_id.as_str(), "client-1");
    assert!(matches!(
        events[0].event.payload,
        AppServerEventPayload::TaskChanged { .. }
    ));
}

#[test]
fn shared_gateway_distinguishes_initialized_event_stream_connections() {
    let gateway = SharedRpcGateway::new(initialized_gateway("client-1", "local-http:client-1"));

    assert!(gateway.connection_is_initialized(&ConnectionId::new("local-http:client-1")));
    assert!(!gateway.connection_is_initialized(&ConnectionId::new("local-http:unknown")));
}

#[test]
fn committed_agent_text_deltas_publish_append_and_chunk_in_order() {
    use openaide_app_server_protocol::ids::MessageId;
    use openaide_app_server_protocol::snapshot::{ChatItem, ChatItemStatus, ChatRole, MessagePart};

    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(2),
    );
    let item = ChatItem {
        message_id: MessageId::from("message-1"),
        turn_id: None,
        role: ChatRole::Agent,
        status: ChatItemStatus::Complete,
        parts: vec![MessagePart::Text {
            text: "first".to_string(),
        }],
    };
    let updates = [
        committed_task_update(
            "task-1",
            2,
            vec![CommittedChatChange::Append { item }],
            Vec::new(),
            TestNavigationChange::None,
        ),
        committed_task_update(
            "task-1",
            3,
            vec![CommittedChatChange::AppendText {
                message_id: MessageId::from("message-1"),
                text: " second".to_string(),
            }],
            Vec::new(),
            TestNavigationChange::None,
        ),
    ];

    let payloads = updates
        .iter()
        .flat_map(|update| gateway.publish_task_update(update, AppServerTime(update.revision)))
        .map(|delivery| delivery.event.payload)
        .collect::<Vec<_>>();

    assert!(
        matches!(
            &payloads[0],
            AppServerEventPayload::TaskChanged { revision, changes, .. }
                if *revision == 2 && matches!(changes.chat.as_slice(),
                    [openaide_app_server_protocol::events::TaskChatChange::Append { item }]
                        if item.status == ChatItemStatus::Complete)
        ),
        "payloads: {payloads:?}"
    );
    assert!(matches!(
        &payloads[1],
        AppServerEventPayload::TaskChanged { revision, changes, .. }
            if *revision == 3 && matches!(changes.chat.as_slice(),
                [openaide_app_server_protocol::events::TaskChatChange::AppendText { text, .. }]
                    if text == " second")
    ));
}

#[test]
fn client_response_error_keeps_pending_server_request() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-1"),
                },
            },
        ),
        AppServerTime(2),
    );
    gateway.open_server_request(task_server_request("task-1"), AppServerTime(3));

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        InboundProtocolMessage::ClientResponse {
            request_id: "server-request-1".to_string(),
            answer: ServerRequestAnswer::Invalid("denied".to_string()),
        },
        AppServerTime(4),
    );

    let error = response_error(outcome);
    assert_eq!(error.error.code, ProtocolErrorCode::ValidationFailed);
    assert_eq!(error.error.message, "denied");
    assert_eq!(
        gateway
            .server_requests
            .pending_for_task(&TaskId::from("task-1"))
            .len(),
        1
    );
}

#[test]
fn unsubscribe_after_initialize_removes_subscription() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    let scope = SubscriptionScope::TaskNavigation { project_id: None };
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: scope.clone(),
            },
        ),
        AppServerTime(2),
    );

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("3", STATE_UNSUBSCRIBE, StateUnsubscribeParams { scope }),
        AppServerTime(3),
    );

    response_value(outcome);
    assert_eq!(gateway.state_stream.subscription_count(), 0);
}

#[test]
fn reinitialized_client_receives_later_events_on_new_connection() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            STATE_SUBSCRIBE,
            StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation { project_id: None },
            },
        ),
        AppServerTime(2),
    );
    gateway.handle_transport_closed(&ConnectionId::new("conn-1"), AppServerTime(3));
    gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request("3", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(4),
    );

    let publish = gateway.state_stream.publish_committed(
        EventScope::StateRoot {
            state_root_id: StateRootId::from("root-1"),
        },
        AppServerEventPayload::TaskNavigationChanged {
            change: openaide_app_server_protocol::events::TaskNavigationChange::Remove {
                task_id: TaskId::from("absent"),
            },
        },
        |client_id| gateway.client_hub.delivery_for(client_id),
        AppServerTime(5),
    );

    assert_eq!(publish.deliveries.len(), 1);
    assert_eq!(
        publish.deliveries[0].delivery.connection_id,
        ConnectionId::new("conn-2")
    );
}

#[test]
fn last_client_expiry_after_reconnect_grace_starts_draining() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    let opened = gateway.open_server_request(client_server_request("client-1"), AppServerTime(2));
    assert!(matches!(opened, OpenRequestOutcome::Opened { .. }));

    gateway.handle_transport_closed(&ConnectionId::new("conn-1"), AppServerTime(3));
    let outcome = gateway.expire_client_after_reconnect_grace(
        &ClientInstanceId::from("client-1"),
        AppServerTime(13),
    );

    assert!(matches!(
        outcome,
        ClientExpiryOutcome::Expired {
            client_instance_id,
            last_client: true,
        } if client_instance_id == ClientInstanceId::from("client-1")
    ));
    assert_eq!(gateway.lifecycle.state(), LifecycleState::Draining);
    assert!(gateway
        .server_requests
        .pending_for_client(&ClientInstanceId::from("client-1"))
        .is_empty());
}

#[test]
fn reattached_client_is_not_expired_by_old_grace_timer() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.handle_transport_closed(&ConnectionId::new("conn-1"), AppServerTime(3));
    gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request("2", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(4),
    );

    let outcome = gateway.expire_client_after_reconnect_grace(
        &ClientInstanceId::from("client-1"),
        AppServerTime(13),
    );

    assert_eq!(outcome, ClientExpiryOutcome::ClientConnected);
    assert_eq!(gateway.lifecycle.state(), LifecycleState::Running);
    assert!(gateway
        .client_hub
        .context_for_connection(&ConnectionId::new("conn-2"))
        .is_some());
}

#[test]
fn heartbeat_refreshes_client_liveness() {
    let mut gateway = initialized_gateway("client-1", "conn-1");

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("2", CLIENT_HEARTBEAT, json!({})),
        AppServerTime(9),
    );

    response_value(outcome);
    assert!(gateway
        .expire_inactive_clients(AppServerTime(10))
        .is_empty());
    assert_eq!(
        gateway.expire_inactive_clients(AppServerTime(19)),
        vec![ClientExpiryOutcome::Expired {
            client_instance_id: ClientInstanceId::from("client-1"),
            last_client: true,
        }]
    );
    assert_eq!(gateway.lifecycle.state(), LifecycleState::Draining);
}

#[test]
fn event_stream_activity_refreshes_client_liveness() {
    let mut gateway = initialized_gateway("client-1", "conn-1");

    assert!(gateway.observe_event_stream_activity(&ConnectionId::new("conn-1"), AppServerTime(9),));

    assert!(gateway
        .expire_inactive_clients(AppServerTime(10))
        .is_empty());
    assert_eq!(
        gateway.expire_inactive_clients(AppServerTime(19)),
        vec![ClientExpiryOutcome::Expired {
            client_instance_id: ClientInstanceId::from("client-1"),
            last_client: true,
        }]
    );
}

#[test]
fn idle_shutdown_waits_when_last_client_expired_but_task_work_is_active() {
    let mut gateway = gateway_with_shutdown(Arc::new(BlockingShutdown {
        active_turns: 1,
        pending_task_requests: 0,
    }));
    initialize(&mut gateway, ConnectionId::new("conn-1"));

    let expired = gateway.expire_inactive_clients(AppServerTime(11));

    assert!(matches!(
        expired.as_slice(),
        [ClientExpiryOutcome::Expired {
            last_client: true,
            ..
        }]
    ));
    assert_eq!(
        gateway.idle_shutdown_decision().unwrap(),
        IdleShutdownDecision::KeepRunning {
            initialized_clients: false,
            blockers: ShutdownBlockers {
                active_turns: 1,
                pending_task_requests: 0,
            },
        }
    );
}

#[test]
fn idle_shutdown_waits_when_a_client_reinitializes_after_expiry() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    let expired = gateway.expire_inactive_clients(AppServerTime(11));
    assert!(matches!(
        expired.as_slice(),
        [ClientExpiryOutcome::Expired {
            last_client: true,
            ..
        }]
    ));

    gateway.handle_inbound(
        ConnectionId::new("conn-2"),
        request("2", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(12),
    );

    assert_eq!(
        gateway.idle_shutdown_decision().unwrap(),
        IdleShutdownDecision::KeepRunning {
            initialized_clients: true,
            blockers: ShutdownBlockers::default(),
        }
    );
}

#[test]
fn idle_shutdown_allows_exit_without_clients_or_task_work() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    let expired = gateway.expire_inactive_clients(AppServerTime(11));
    assert!(matches!(
        expired.as_slice(),
        [ClientExpiryOutcome::Expired {
            last_client: true,
            ..
        }]
    ));

    assert_eq!(
        gateway.idle_shutdown_decision().unwrap(),
        IdleShutdownDecision::ShutdownNow
    );
}

#[test]
fn inactive_expiry_interrupts_client_scoped_requests() {
    let mut gateway = initialized_gateway("client-1", "conn-1");
    gateway.open_server_request(client_server_request("client-1"), AppServerTime(2));

    let expired = gateway.expire_inactive_clients(AppServerTime(11));

    assert_eq!(
        expired,
        vec![ClientExpiryOutcome::Expired {
            client_instance_id: ClientInstanceId::from("client-1"),
            last_client: true,
        }]
    );
    assert!(gateway
        .server_requests
        .pending_for_client(&ClientInstanceId::from("client-1"))
        .is_empty());
}

#[test]
fn attachment_reveal_opens_same_client_shell_reveal_request_with_opaque_handle() {
    let mut gateway = gateway_with_attachments(Arc::new(RevealAttachments));
    gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request("1", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(1),
    );

    let outcome = gateway.handle_inbound(
        ConnectionId::new("conn-1"),
        request(
            "2",
            ATTACHMENT_REVEAL,
            openaide_app_server_protocol::attachment::AttachmentRevealParams {
                task_id: TaskId::from("task-1"),
                handle_id: "attachment-handle-1".into(),
            },
        ),
        AppServerTime(2),
    );

    let (value, server_requests) = response_value_and_server_requests(outcome);
    assert_eq!(value["result"]["requested"], json!(true));
    assert_eq!(server_requests.len(), 1);
    assert_eq!(
        server_requests[0].delivery.connection_id,
        ConnectionId::new("conn-1")
    );
    assert_eq!(server_requests[0].envelope.method, "shell/revealFile");
    assert_eq!(server_requests[0].envelope.params["label"], "notes.md");
    assert_eq!(
        server_requests[0].envelope.params["originatingClientInstanceId"],
        "client-1"
    );
    let file_handle_id = server_requests[0].envelope.params["fileHandleId"]
        .as_str()
        .unwrap();
    let token = file_handle_id.strip_prefix("file-reveal-").unwrap();
    assert!(uuid::Uuid::parse_str(token).is_ok());
    assert!(server_requests[0].envelope.params.get("path").is_none());
}

#[test]
fn native_shell_resolves_only_the_originating_clients_reveal_handle_once() {
    let mut gateway = gateway_with_attachments(Arc::new(RevealAttachments));
    let origin_connection = ConnectionId::new("origin-connection");
    gateway.handle_inbound(
        origin_connection.clone(),
        request("1", CLIENT_INITIALIZE, init_params("origin-client")),
        AppServerTime(1),
    );
    let reveal = gateway.handle_inbound(
        origin_connection.clone(),
        request(
            "2",
            ATTACHMENT_REVEAL,
            openaide_app_server_protocol::attachment::AttachmentRevealParams {
                task_id: TaskId::from("task-1"),
                handle_id: "attachment-handle-1".into(),
            },
        ),
        AppServerTime(2),
    );
    let (_, server_requests) = response_value_and_server_requests(reveal);
    let reveal_params = server_requests[0].envelope.params.clone();

    let denied = gateway.handle_inbound(
        origin_connection,
        request("3", SHELL_RESOLVE_FILE_REVEAL, reveal_params.clone()),
        AppServerTime(3),
    );
    assert_eq!(
        response_error(denied).error.code,
        ProtocolErrorCode::CapabilityUnavailable
    );

    let mut web_resolver = init_params("web-resolver");
    web_resolver.capabilities.shell = vec![ShellCapability::ResolveFileReveal];
    let web_resolver_connection = ConnectionId::new("web-resolver-connection");
    gateway.handle_inbound(
        web_resolver_connection.clone(),
        request("4", CLIENT_INITIALIZE, web_resolver),
        AppServerTime(4),
    );
    let denied = gateway.handle_inbound(
        web_resolver_connection,
        request("5", SHELL_RESOLVE_FILE_REVEAL, reveal_params.clone()),
        AppServerTime(5),
    );
    assert_eq!(
        response_error(denied).error.code,
        ProtocolErrorCode::CapabilityUnavailable
    );

    let mut native_host = init_params("native-host");
    native_host.shell.kind = ShellKind::VscodeExtension;
    native_host.capabilities.shell = vec![ShellCapability::ResolveFileReveal];
    let host_connection = ConnectionId::new("host-connection");
    gateway.handle_inbound(
        host_connection.clone(),
        request("6", CLIENT_INITIALIZE, native_host),
        AppServerTime(6),
    );
    let wrong_origin = gateway.handle_inbound(
        host_connection.clone(),
        request(
            "7",
            SHELL_RESOLVE_FILE_REVEAL,
            json!({
                "originatingClientInstanceId": "another-client",
                "fileHandleId": reveal_params["fileHandleId"],
            }),
        ),
        AppServerTime(7),
    );
    assert_eq!(
        response_error(wrong_origin).error.code,
        ProtocolErrorCode::CapabilityUnavailable
    );

    let resolved = response_value(gateway.handle_inbound(
        host_connection.clone(),
        request("8", SHELL_RESOLVE_FILE_REVEAL, reveal_params.clone()),
        AppServerTime(8),
    ));
    assert_eq!(resolved["result"]["path"], "/workspace/app/notes.md");
    assert_eq!(resolved["result"]["label"], "notes.md");

    let consumed = gateway.handle_inbound(
        host_connection,
        request("9", SHELL_RESOLVE_FILE_REVEAL, reveal_params),
        AppServerTime(9),
    );
    assert_eq!(
        response_error(consumed).error.code,
        ProtocolErrorCode::CapabilityUnavailable
    );
}

fn gateway() -> RpcGateway {
    gateway_with_attachments(Arc::new(RejectingAttachments))
}

fn gateway_with_project_context() -> RpcGateway {
    gateway_with_project_context_and_store().0
}

fn gateway_with_project_context_and_store() -> (RpcGateway, Store) {
    let root = tempfile::tempdir().unwrap().keep();
    let store = Store::open(root).unwrap();
    let project_roots = crate::projects::ConfiguredProjectRoots::default();
    let task_snapshots = Arc::new(TaskSnapshotStore::new(store.clone()));
    let snapshots = SnapshotBuilder::with_sources(
        "server-1".into(),
        "root-1".into(),
        SnapshotSources::new(
            Arc::new(store.clone()),
            Arc::new(AgentRegistrySnapshotSource::new(
                crate::agent::registry::AgentRegistry::default_built_ins(),
            )),
            Arc::new(ProjectCollectionStore::new_with_configured_roots(
                store.clone(),
                project_roots.clone(),
            )),
            Arc::new(crate::worktrees::WorktreeManager::new(store.clone())),
            Arc::new(SettingsCatalog::default()),
            Arc::new(TaskNavigationStore::new(store.clone())),
            task_snapshots.clone(),
        ),
    );
    let gateway = RpcGateway::new(
        ClientHub::new(10),
        AppLifecycle::new(),
        StateStream::new(StateRootId::from("root-1")),
        ServerRequestRuntime::new(),
        ShellFileRevealRegistry::new(),
        snapshots,
        task_snapshots,
        project_roots,
        AppServerProbeFacts::new("root-1"),
        runtime_diagnostics(),
        Arc::new(RejectingAgentProbe),
        Arc::new(RejectingAgentAuthenticate),
        Arc::new(RejectingAgentCatalogMutations),
        Arc::new(RejectingAgentSettingsDetails),
        Arc::new(McpServersSettingsService::new()),
        Arc::new(SkillsSettingsService::new()),
        app_preferences(),
        runtime_settings(),
        Arc::new(RejectingAgentListSessions),
        Arc::new(RejectingAttachments),
        Arc::new(RejectingTaskAcquire),
        Arc::new(RejectingTaskFileSearch),
        Arc::new(RejectingTaskAdoptNativeSession),
        Arc::new(RejectingTaskSend),
        Arc::new(RejectingTaskCancel),
        Arc::new(RejectingTaskOpen),
        Arc::new(RejectingTaskChatPage),
        Arc::new(RejectingTaskSetConfigOption),
        Arc::new(RejectingTaskRelease),
        Arc::new(RejectingTaskArchive),
        Arc::new(crate::worktrees::WorktreeManager::new(store.clone())),
        Arc::new(FixedShutdown),
    );
    (gateway, store)
}

fn gateway_with_attachments(attachments: Arc<dyn AttachmentFileBrowserWorkflow>) -> RpcGateway {
    gateway_with_attachments_and_shutdown(attachments, Arc::new(FixedShutdown))
}

fn gateway_with_shutdown(shutdown: Arc<dyn AppServerShutdownWorkflow>) -> RpcGateway {
    gateway_with_attachments_and_shutdown(Arc::new(RejectingAttachments), shutdown)
}

fn gateway_with_attachments_and_shutdown(
    attachments: Arc<dyn AttachmentFileBrowserWorkflow>,
    shutdown: Arc<dyn AppServerShutdownWorkflow>,
) -> RpcGateway {
    RpcGateway::new(
        ClientHub::new(10),
        AppLifecycle::new(),
        StateStream::new(StateRootId::from("root-1")),
        ServerRequestRuntime::new(),
        ShellFileRevealRegistry::new(),
        SnapshotBuilder::new("server-1".into(), "root-1".into()),
        std::sync::Arc::new(EmptyTaskSnapshots),
        crate::projects::ConfiguredProjectRoots::default(),
        AppServerProbeFacts::new("root-1"),
        runtime_diagnostics(),
        std::sync::Arc::new(RejectingAgentProbe),
        std::sync::Arc::new(RejectingAgentAuthenticate),
        std::sync::Arc::new(RejectingAgentCatalogMutations),
        std::sync::Arc::new(RejectingAgentSettingsDetails),
        std::sync::Arc::new(McpServersSettingsService::new()),
        std::sync::Arc::new(SkillsSettingsService::new()),
        app_preferences(),
        runtime_settings(),
        std::sync::Arc::new(RejectingAgentListSessions),
        attachments,
        std::sync::Arc::new(RejectingTaskAcquire),
        std::sync::Arc::new(RejectingTaskFileSearch),
        std::sync::Arc::new(RejectingTaskAdoptNativeSession),
        std::sync::Arc::new(RejectingTaskSend),
        std::sync::Arc::new(RejectingTaskCancel),
        std::sync::Arc::new(RejectingTaskOpen),
        std::sync::Arc::new(FixedTaskChatPage),
        std::sync::Arc::new(RejectingTaskSetConfigOption),
        std::sync::Arc::new(RejectingTaskRelease),
        std::sync::Arc::new(RejectingTaskArchive),
        test_worktrees(),
        shutdown,
    )
}

fn runtime_settings() -> Arc<RuntimeSettingsService> {
    Arc::new(RuntimeSettingsService::new(
        crate::agent::acp_trace::AcpTraceState::disabled(std::path::Path::new(".")),
    ))
}

fn app_preferences() -> Arc<AppPreferencesService> {
    let dir = tempfile::tempdir().unwrap().keep();
    let store = crate::storage::Store::open(dir).unwrap();
    Arc::new(AppPreferencesService::new(store))
}

fn test_worktrees() -> Arc<crate::worktrees::WorktreeManager> {
    let dir = tempfile::tempdir().unwrap().keep();
    Arc::new(crate::worktrees::WorktreeManager::new(
        crate::storage::Store::open(dir).unwrap(),
    ))
}

fn runtime_diagnostics() -> Arc<FixedRuntimeDiagnostics> {
    Arc::new(FixedRuntimeDiagnostics)
}

struct FixedRuntimeDiagnostics;

impl RuntimeDiagnosticsWorkflow for FixedRuntimeDiagnostics {
    fn runtime_diagnostics(
        &self,
    ) -> Result<
        openaide_app_server_protocol::diagnostics::RuntimeDiagnosticsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Ok(openaide_app_server_protocol::diagnostics::RuntimeDiagnosticsResult {
            status: openaide_app_server_protocol::diagnostics::RuntimeDiagnosticsStatus::Ready,
            version: Some("0.1.0-test".to_string()),
            method_count: openaide_app_server_protocol::methods::CLIENT_METHODS.len(),
            tasks: openaide_app_server_protocol::diagnostics::TaskDiagnosticsResult {
                visible_count: 2,
                total_count: 3,
                active_count: 1,
                active_tasks: Vec::new(),
                revision: 9,
            },
            redaction: openaide_app_server_protocol::diagnostics::DiagnosticsRedaction::PromptTextFileContentsTerminalOutputAndSecretsRemoved,
        })
    }
}

fn gateway_with_agent_session_listing(
    agent_list_sessions: Arc<dyn AgentListSessionsWorkflow>,
) -> RpcGateway {
    RpcGateway::new(
        ClientHub::new(10),
        AppLifecycle::new(),
        StateStream::new(StateRootId::from("root-1")),
        ServerRequestRuntime::new(),
        ShellFileRevealRegistry::new(),
        SnapshotBuilder::new("server-1".into(), "root-1".into()),
        std::sync::Arc::new(EmptyTaskSnapshots),
        crate::projects::ConfiguredProjectRoots::default(),
        AppServerProbeFacts::new("root-1"),
        runtime_diagnostics(),
        std::sync::Arc::new(RejectingAgentProbe),
        std::sync::Arc::new(RejectingAgentAuthenticate),
        std::sync::Arc::new(RejectingAgentCatalogMutations),
        std::sync::Arc::new(RejectingAgentSettingsDetails),
        std::sync::Arc::new(McpServersSettingsService::new()),
        std::sync::Arc::new(SkillsSettingsService::new()),
        app_preferences(),
        runtime_settings(),
        agent_list_sessions,
        Arc::new(RejectingAttachments),
        std::sync::Arc::new(RejectingTaskAcquire),
        std::sync::Arc::new(RejectingTaskFileSearch),
        std::sync::Arc::new(RejectingTaskAdoptNativeSession),
        std::sync::Arc::new(RejectingTaskSend),
        std::sync::Arc::new(RejectingTaskCancel),
        std::sync::Arc::new(RejectingTaskOpen),
        std::sync::Arc::new(RejectingTaskChatPage),
        std::sync::Arc::new(RejectingTaskSetConfigOption),
        std::sync::Arc::new(RejectingTaskRelease),
        std::sync::Arc::new(RejectingTaskArchive),
        test_worktrees(),
        Arc::new(FixedShutdown),
    )
}

fn gateway_with_agent_authenticate(
    agent_authenticate: Arc<dyn AgentAuthenticateWorkflow>,
) -> RpcGateway {
    RpcGateway::new(
        ClientHub::new(10),
        AppLifecycle::new(),
        StateStream::new(StateRootId::from("root-1")),
        ServerRequestRuntime::new(),
        ShellFileRevealRegistry::new(),
        SnapshotBuilder::new("server-1".into(), "root-1".into()),
        std::sync::Arc::new(EmptyTaskSnapshots),
        crate::projects::ConfiguredProjectRoots::default(),
        AppServerProbeFacts::new("root-1"),
        runtime_diagnostics(),
        std::sync::Arc::new(RejectingAgentProbe),
        agent_authenticate,
        std::sync::Arc::new(RejectingAgentCatalogMutations),
        std::sync::Arc::new(RejectingAgentSettingsDetails),
        std::sync::Arc::new(McpServersSettingsService::new()),
        std::sync::Arc::new(SkillsSettingsService::new()),
        app_preferences(),
        runtime_settings(),
        Arc::new(RejectingAgentListSessions),
        Arc::new(RejectingAttachments),
        std::sync::Arc::new(RejectingTaskAcquire),
        std::sync::Arc::new(RejectingTaskFileSearch),
        std::sync::Arc::new(RejectingTaskAdoptNativeSession),
        std::sync::Arc::new(RejectingTaskSend),
        std::sync::Arc::new(RejectingTaskCancel),
        std::sync::Arc::new(RejectingTaskOpen),
        std::sync::Arc::new(RejectingTaskChatPage),
        std::sync::Arc::new(RejectingTaskSetConfigOption),
        std::sync::Arc::new(RejectingTaskRelease),
        std::sync::Arc::new(RejectingTaskArchive),
        test_worktrees(),
        Arc::new(FixedShutdown),
    )
}

struct FixedShutdown;

impl AppServerShutdownWorkflow for FixedShutdown {
    fn shutdown(&self) -> Result<(), crate::protocol::errors::RuntimeError> {
        Ok(())
    }

    fn shutdown_blockers(
        &self,
    ) -> Result<crate::protocol_edge::ShutdownBlockers, crate::protocol::errors::RuntimeError> {
        Ok(crate::protocol_edge::ShutdownBlockers::default())
    }
}

struct BlockingShutdown {
    active_turns: usize,
    pending_task_requests: usize,
}

impl AppServerShutdownWorkflow for BlockingShutdown {
    fn shutdown(&self) -> Result<(), crate::protocol::errors::RuntimeError> {
        Ok(())
    }

    fn shutdown_blockers(
        &self,
    ) -> Result<crate::protocol_edge::ShutdownBlockers, crate::protocol::errors::RuntimeError> {
        Ok(crate::protocol_edge::ShutdownBlockers {
            active_turns: self.active_turns,
            pending_task_requests: self.pending_task_requests,
        })
    }
}

struct RejectingAgentProbe;

impl AgentProbeWorkflow for RejectingAgentProbe {
    fn probe(
        &self,
        _params: openaide_app_server_protocol::agent::AgentProbeParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentProbeResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "agent probe unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct RejectingAgentAuthenticate;

impl AgentAuthenticateWorkflow for RejectingAgentAuthenticate {
    fn authenticate(
        &self,
        _params: openaide_app_server_protocol::agent::AgentAuthenticateParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentAuthenticateResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "agent authentication unavailable in test gateway",
        ))
    }
}

struct RejectingAgentSettingsDetails;

impl AgentSettingsDetailsWorkflow for RejectingAgentSettingsDetails {
    fn agent_settings_details(
        &self,
        _params: openaide_app_server_protocol::agent::AgentSettingsDetailsParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentSettingsDetailsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "agent settings details unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct RejectingAgentListSessions;

impl AgentListSessionsWorkflow for RejectingAgentListSessions {
    fn list_agent_sessions(
        &self,
        _params: openaide_app_server_protocol::agent::AgentListSessionsParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentListSessionsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "agent session listing unavailable in test gateway",
        ))
    }
}

struct ListingAgentSessions;

impl AgentListSessionsWorkflow for ListingAgentSessions {
    fn list_agent_sessions(
        &self,
        params: openaide_app_server_protocol::agent::AgentListSessionsParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentListSessionsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Ok(
            openaide_app_server_protocol::agent::AgentListSessionsResult {
                agent_id: params.agent_id,
                project_id: params.project_id,
                project_label: "Workspace".to_string(),
                sessions: vec![openaide_app_server_protocol::agent::AgentListedSession {
                    session_id: "session-1".to_string(),
                    title: Some("Session".to_string()),
                    last_activity: Some("2026-05-18T00:00:00Z".to_string()),
                    updated_at: Some("2026-05-18T00:00:00Z".to_string()),
                }],
                next_cursor: params.cursor.map(|_| "cursor-2".to_string()),
            },
        )
    }
}

#[derive(Default)]
struct RecordingCatalogRefresh {
    requests: AtomicUsize,
}

impl AgentListSessionsWorkflow for RecordingCatalogRefresh {
    fn list_agent_sessions(
        &self,
        _params: openaide_app_server_protocol::agent::AgentListSessionsParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentListSessionsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable("interactive listing is not used"))
    }

    fn request_native_session_catalog_refresh(&self) {
        self.requests.fetch_add(1, Ordering::SeqCst);
    }
}

struct AuthenticatingAgent;

impl AgentAuthenticateWorkflow for AuthenticatingAgent {
    fn authenticate(
        &self,
        params: openaide_app_server_protocol::agent::AgentAuthenticateParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentAuthenticateResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Ok(
            openaide_app_server_protocol::agent::AgentAuthenticateResult {
                agent_id: params.agent_id,
                method_id: params.method_id,
                status: openaide_app_server_protocol::agent::AgentAuthenticateStatus::Authenticated,
                agents: openaide_app_server_protocol::snapshot::AgentCollectionSnapshot {
                    agents: Vec::new(),
                },
            },
        )
    }
}

struct RejectingAttachments;

impl AttachmentFileBrowserWorkflow for RejectingAttachments {
    fn keep_alive_for_client(&self, _client_instance_id: &ClientInstanceId) {}

    fn discard_resources_for_client(&self, _client_instance_id: &ClientInstanceId) {}

    fn list_roots(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentListRootsParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentListRootsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment roots unavailable in test gateway",
        ))
    }

    fn list_directory(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentListDirectoryParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentListDirectoryResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment directory unavailable in test gateway",
        ))
    }

    fn create_file_reference(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentCreateFileReferenceParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentCreateFileReferenceResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment file reference unavailable in test gateway",
        ))
    }

    fn create_pasted_image(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentCreatePastedImageParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentCreatePastedImageResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment pasted image unavailable in test gateway",
        ))
    }

    fn create_embedded_candidate(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentCreateEmbeddedCandidateParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentCreateEmbeddedCandidateResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment embedded candidate unavailable in test gateway",
        ))
    }

    fn confirm_embedded(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentConfirmEmbeddedParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentConfirmEmbeddedResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment embedded confirmation unavailable in test gateway",
        ))
    }

    fn refresh_handles(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentRefreshHandlesParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentRefreshHandlesResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment refresh unavailable in test gateway",
        ))
    }

    fn release_resources(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentReleaseParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentReleaseResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "attachment release unavailable in test gateway",
        ))
    }

    fn resolve_reveal_target(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentRevealParams,
    ) -> Result<ResolvedRevealAttachment, openaide_app_server_protocol::errors::ProtocolError> {
        Err(test_unavailable(
            "attachment reveal unavailable in test gateway",
        ))
    }

    fn workspace_roots(
        &self,
        _params: openaide_app_server_protocol::workspace::WorkspaceListRootsParams,
    ) -> Result<
        openaide_app_server_protocol::workspace::WorkspaceListRootsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "workspace roots unavailable in test gateway",
        ))
    }

    fn workspace_directory(
        &self,
        _params: openaide_app_server_protocol::workspace::WorkspaceListDirectoryParams,
    ) -> Result<
        openaide_app_server_protocol::workspace::WorkspaceListDirectoryResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(test_unavailable(
            "workspace directory unavailable in test gateway",
        ))
    }
}

struct RevealAttachments;

impl AttachmentFileBrowserWorkflow for RevealAttachments {
    fn keep_alive_for_client(&self, client_instance_id: &ClientInstanceId) {
        RejectingAttachments.keep_alive_for_client(client_instance_id);
    }

    fn discard_resources_for_client(&self, client_instance_id: &ClientInstanceId) {
        RejectingAttachments.discard_resources_for_client(client_instance_id);
    }

    fn list_roots(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentListRootsParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentListRootsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.list_roots(client_instance_id, params)
    }

    fn list_directory(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentListDirectoryParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentListDirectoryResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.list_directory(client_instance_id, params)
    }

    fn create_file_reference(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentCreateFileReferenceParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentCreateFileReferenceResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.create_file_reference(client_instance_id, params)
    }

    fn create_pasted_image(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentCreatePastedImageParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentCreatePastedImageResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.create_pasted_image(client_instance_id, params)
    }

    fn create_embedded_candidate(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentCreateEmbeddedCandidateParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentCreateEmbeddedCandidateResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.create_embedded_candidate(client_instance_id, params)
    }

    fn confirm_embedded(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentConfirmEmbeddedParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentConfirmEmbeddedResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.confirm_embedded(client_instance_id, params)
    }

    fn refresh_handles(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentRefreshHandlesParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentRefreshHandlesResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.refresh_handles(client_instance_id, params)
    }

    fn release_resources(
        &self,
        client_instance_id: &ClientInstanceId,
        params: openaide_app_server_protocol::attachment::AttachmentReleaseParams,
    ) -> Result<
        openaide_app_server_protocol::attachment::AttachmentReleaseResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.release_resources(client_instance_id, params)
    }

    fn resolve_reveal_target(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::attachment::AttachmentRevealParams,
    ) -> Result<ResolvedRevealAttachment, openaide_app_server_protocol::errors::ProtocolError> {
        Ok(ResolvedRevealAttachment {
            path: PathBuf::from("/workspace/app/notes.md"),
            label: "notes.md".to_string(),
        })
    }

    fn workspace_roots(
        &self,
        params: openaide_app_server_protocol::workspace::WorkspaceListRootsParams,
    ) -> Result<
        openaide_app_server_protocol::workspace::WorkspaceListRootsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.workspace_roots(params)
    }

    fn workspace_directory(
        &self,
        params: openaide_app_server_protocol::workspace::WorkspaceListDirectoryParams,
    ) -> Result<
        openaide_app_server_protocol::workspace::WorkspaceListDirectoryResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        RejectingAttachments.workspace_directory(params)
    }
}

fn test_unavailable(message: &str) -> openaide_app_server_protocol::errors::ProtocolError {
    openaide_app_server_protocol::errors::ProtocolError {
        code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}

struct RejectingAgentCatalogMutations;

impl AgentCatalogMutationWorkflow for RejectingAgentCatalogMutations {
    fn create_custom(
        &self,
        _params: openaide_app_server_protocol::agent::AgentCreateCustomParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentCreateCustomResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(rejecting_agent_catalog_error())
    }

    fn update_custom_metadata(
        &self,
        _params: openaide_app_server_protocol::agent::AgentUpdateCustomMetadataParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentUpdateCustomMetadataResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(rejecting_agent_catalog_error())
    }

    fn replace_custom(
        &self,
        _params: openaide_app_server_protocol::agent::AgentReplaceCustomParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentReplaceCustomResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(rejecting_agent_catalog_error())
    }

    fn delete_custom(
        &self,
        _params: openaide_app_server_protocol::agent::AgentDeleteCustomParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentDeleteCustomResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(rejecting_agent_catalog_error())
    }

    fn set_enabled(
        &self,
        _params: openaide_app_server_protocol::agent::AgentSetEnabledParams,
    ) -> Result<
        openaide_app_server_protocol::agent::AgentSetEnabledResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(rejecting_agent_catalog_error())
    }
}

fn rejecting_agent_catalog_error() -> openaide_app_server_protocol::errors::ProtocolError {
    openaide_app_server_protocol::errors::ProtocolError {
        code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
        message: "agent catalog mutations unavailable in test gateway".to_string(),
        recoverable: true,
        target: None,
    }
}

struct EmptyTaskSnapshots;

impl TaskSnapshotSource for EmptyTaskSnapshots {
    fn list(
        &self,
        _archived: bool,
        _project_id: Option<&openaide_app_server_protocol::ids::ProjectId>,
        _cursor: Option<&openaide_app_server_protocol::ids::TaskListCursor>,
    ) -> Result<TaskListSnapshot, openaide_app_server_protocol::errors::ProtocolError> {
        Ok(TaskListSnapshot {
            tasks: Vec::new(),
            revision: 0,
            next_cursor: None,
        })
    }

    fn open_internal(
        &self,
        task_id: &openaide_app_server_protocol::ids::TaskId,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::NotFound,
            message: format!("task not found: {}", task_id.as_str()),
            recoverable: false,
            target: None,
        })
    }

    fn open_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        task_id: &openaide_app_server_protocol::ids::TaskId,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        self.open_internal(task_id)
    }

    fn tool_detail_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
        _artifact_id: &str,
    ) -> Result<
        openaide_app_server_protocol::task::ToolDetailSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Ok(fixed_tool_detail())
    }
}

struct RejectingTaskAcquire;

impl TaskAcquireWorkflow for RejectingTaskAcquire {
    fn acquire_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskAcquireParams,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task acquire unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct RejectingTaskFileSearch;

impl TaskFileSearchWorkflow for RejectingTaskFileSearch {
    fn search_files_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskSearchFilesParams,
    ) -> Result<
        openaide_app_server_protocol::task::TaskSearchFilesResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task file search unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct RejectingTaskSend;

impl TaskSendWorkflow for RejectingTaskSend {
    fn send_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskSendParams,
    ) -> Result<TaskSendAccepted, openaide_app_server_protocol::errors::ProtocolError> {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task send unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct RejectingTaskCancel;

struct RejectingTaskAdoptNativeSession;

impl TaskAdoptNativeSessionWorkflow for RejectingTaskAdoptNativeSession {
    fn adopt_native_session(
        &self,
        _params: openaide_app_server_protocol::task::TaskAdoptNativeSessionParams,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task native session adoption unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

impl TaskCancelWorkflow for RejectingTaskCancel {
    fn cancel_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskCancelParams,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task cancel unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn recover_stuck_sessions(
        &self,
        _params: openaide_app_server_protocol::support::SupportRecoverStuckSessionsParams,
    ) -> Result<
        openaide_app_server_protocol::support::SupportRecoverStuckSessionsResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "support recovery unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct RejectingTaskOpen;

impl TaskOpenWorkflow for RejectingTaskOpen {
    fn mark_read_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskMarkReadParams,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task mark-read unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn open_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskOpenParams,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task open unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct FixedTaskChatPage;

impl TaskChatPageWorkflow for FixedTaskChatPage {
    fn chat_page_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskChatPageParams,
    ) -> Result<
        openaide_app_server_protocol::task::TaskChatPageResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Ok(openaide_app_server_protocol::task::TaskChatPageResult {
            task_id: "task-1".into(),
            items: vec![openaide_app_server_protocol::snapshot::ChatItem {
                message_id: "msg-1".into(),
                turn_id: None,
                role: openaide_app_server_protocol::snapshot::ChatRole::Agent,
                status: openaide_app_server_protocol::snapshot::ChatItemStatus::Complete,
                parts: vec![openaide_app_server_protocol::snapshot::MessagePart::Text {
                    text: "older".to_string(),
                }],
            }],
            has_before: false,
            total_count: 1,
            revision: 7,
            start_cursor: Some("msg-1".into()),
            end_cursor: Some("msg-1".into()),
        })
    }
}

struct RejectingTaskSetConfigOption;

struct RejectingTaskChatPage;

impl TaskChatPageWorkflow for RejectingTaskChatPage {
    fn chat_page_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskChatPageParams,
    ) -> Result<
        openaide_app_server_protocol::task::TaskChatPageResult,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task chat page unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

fn fixed_tool_detail() -> openaide_app_server_protocol::task::ToolDetailSnapshot {
    openaide_app_server_protocol::task::ToolDetailSnapshot {
        locations: vec![openaide_app_server_protocol::task::ActivityToolLocation {
            path: "src/main.rs".to_string(),
            line: Some(12),
        }],
        content: vec![
            openaide_app_server_protocol::task::ActivityToolContent::Text {
                text: "details".to_string(),
            },
        ],
        input: Some(openaide_app_server_protocol::task::ActivityToolInput {
            command: vec!["cargo".to_string(), "test".to_string()],
            cwd: Some("workspace".to_string()),
            query: None,
            queries: None,
            url: None,
            path: None,
            fields: vec![openaide_app_server_protocol::task::ActivityToolField {
                name: "mode".to_string(),
                value: openaide_app_server_protocol::task::ActivityToolValue::String {
                    value: "check".to_string(),
                },
            }],
        }),
        output: Some(openaide_app_server_protocol::task::ActivityToolOutput {
            stdout: Some("ok".to_string()),
            stderr: None,
            formatted_output: None,
            aggregated_output: None,
            exit_code: Some(0),
            success: Some(true),
            fields: Vec::new(),
        }),
    }
}

impl TaskSetConfigOptionWorkflow for RejectingTaskSetConfigOption {
    fn set_config_option_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskSetConfigOptionParams,
    ) -> Result<
        openaide_app_server_protocol::snapshot::TaskSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task set config option unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

struct RejectingTaskRelease;

impl TaskReleaseWorkflow for RejectingTaskRelease {
    fn release_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskReleaseParams,
    ) -> Result<(), openaide_app_server_protocol::errors::ProtocolError> {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task release unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn release_expired_client(
        &self,
        _client_instance_id: &ClientInstanceId,
    ) -> Result<(), openaide_app_server_protocol::errors::ProtocolError> {
        Ok(())
    }

    fn dispose_prepared_tasks_for_agent(
        &self,
        _agent_id: &str,
    ) -> Result<(), openaide_app_server_protocol::errors::ProtocolError> {
        Ok(())
    }
}

struct RejectingTaskArchive;

impl TaskArchiveWorkflow for RejectingTaskArchive {
    fn set_archived_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _params: openaide_app_server_protocol::task::TaskSetArchivedParams,
    ) -> Result<(), openaide_app_server_protocol::errors::ProtocolError> {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::Internal,
            message: "task archive unavailable in test gateway".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

fn initialized_gateway(client_id: &str, connection_id: &str) -> RpcGateway {
    let mut gateway = gateway();
    gateway.handle_inbound(
        ConnectionId::new(connection_id),
        request("1", CLIENT_INITIALIZE, init_params(client_id)),
        AppServerTime(1),
    );
    gateway
}

fn client_new_task_record(
    task_id: &str,
    owner_client_instance_id: &str,
) -> crate::storage::records::TaskRecord {
    crate::storage::records::TaskRecord {
        task_id: task_id.to_string(),
        title: None,
        status: crate::protocol::model::TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: crate::protocol::model::IsolationKind::Local,
        workspace_root: "/workspace/app".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::New {
            lease: Some(ClientInstanceId::from(owner_client_instance_id)),
        },
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options: Default::default(),
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: crate::storage::records::TaskPreparationRecord::Ready,
    }
}

fn initialize(gateway: &mut RpcGateway, connection_id: ConnectionId) {
    gateway.handle_inbound(
        connection_id,
        request("1", CLIENT_INITIALIZE, init_params("client-1")),
        AppServerTime(1),
    );
}

fn request<T: serde::Serialize>(id: &str, method: &str, params: T) -> InboundProtocolMessage {
    request_with_meta(id, method, params, RequestMeta::default())
}

fn request_with_meta<T: serde::Serialize>(
    id: &str,
    method: &str,
    params: T,
    meta: RequestMeta,
) -> InboundProtocolMessage {
    InboundProtocolMessage::ClientRequest {
        id: id.to_string(),
        method: method.to_string(),
        params: serde_json::to_value(params).unwrap(),
        meta,
    }
}

fn init_params(client_id: &str) -> InitializeParams {
    InitializeParams {
        client_instance_id: ClientInstanceId::from(client_id),
        shell: ShellDescriptor {
            kind: ShellKind::Web,
            name: None,
            version: None,
        },
        requested_surface: RequestedSurface::Home,
        capabilities: ClientCapabilities {
            protocol: vec![
                ClientProtocolCapability::PermissionResponses,
                ClientProtocolCapability::QuestionResponses,
            ],
            shell: Vec::new(),
        },
        workspace_roots: Vec::new(),
    }
}

fn init_params_without_request_responses(client_id: &str) -> InitializeParams {
    InitializeParams {
        capabilities: ClientCapabilities::default(),
        ..init_params(client_id)
    }
}

fn response_value(outcome: GatewayOutcome) -> serde_json::Value {
    match outcome {
        GatewayOutcome::Respond {
            response: GatewayResponse::Result(value),
            ..
        } => value,
        other => panic!("expected result response, got {other:?}"),
    }
}

fn response_value_and_server_requests(
    outcome: GatewayOutcome,
) -> (
    serde_json::Value,
    Vec<crate::server_requests::ServerRequestDelivery>,
) {
    match outcome {
        GatewayOutcome::Respond {
            response: GatewayResponse::Result(value),
            server_requests,
            ..
        } => (value, server_requests),
        other => panic!("expected result response, got {other:?}"),
    }
}

fn response_events(outcome: GatewayOutcome) -> Vec<GatewayEventDelivery> {
    match outcome {
        GatewayOutcome::Respond { events, .. } => events,
        other => panic!("expected response, got {other:?}"),
    }
}

fn response_error(outcome: GatewayOutcome) -> ErrorEnvelope {
    match outcome {
        GatewayOutcome::Respond {
            response: GatewayResponse::Error(error),
            ..
        } => *error,
        other => panic!("expected error response, got {other:?}"),
    }
}

fn committed_task_update(
    task_id: &str,
    revision: u64,
    chat: Vec<CommittedChatChange>,
    tool_details: Vec<ToolDetailUpdate>,
    navigation: TestNavigationChange,
) -> TaskUpdate {
    use openaide_app_server_protocol::events::{TaskChanges, TaskChatChange, TaskNavigationChange};
    use openaide_app_server_protocol::snapshot::{ChatSnapshot, TaskStatus, TaskSummary};

    let chat = chat
        .into_iter()
        .map(|change| match change {
            CommittedChatChange::Append { item } => TaskChatChange::Append { item },
            CommittedChatChange::Upsert { item } => TaskChatChange::Upsert { item },
            CommittedChatChange::AppendText { message_id, text } => {
                TaskChatChange::AppendText { message_id, text }
            }
            CommittedChatChange::Replace => TaskChatChange::Replace {
                chat: ChatSnapshot {
                    items: Vec::new(),
                    has_more_before: false,
                    has_messages: false,
                    start_cursor: None,
                    end_cursor: None,
                },
            },
        })
        .collect();
    let navigation = match navigation {
        TestNavigationChange::None => None,
        TestNavigationChange::Upsert => Some(TaskNavigationChange::Upsert {
            task: Box::new(TaskSummary {
                task_id: task_id.into(),
                project_id: "project-1".into(),
                agent_id: "codex".into(),
                title: None,
                status: TaskStatus::Idle,
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                last_activity: "2026-01-01T00:00:00Z".to_string(),
                unread: false,
                attention: None,
                has_messages: true,
                worktree_id: None,
                workspace_available: true,
            }),
        }),
    };
    TaskUpdate {
        task_id: task_id.to_string(),
        revision,
        kind: TaskUpdateKind::Changed(Box::new(CommittedTaskChange {
            changes: TaskChanges {
                chat,
                ..TaskChanges::default()
            },
            tool_details,
            navigation,
        })),
    }
}

enum TestNavigationChange {
    None,
    Upsert,
}

fn task_server_request(task_id: &str) -> ServerRequestDraft {
    ServerRequestDraft {
        scope: PendingRequestScope::Task {
            task_id: TaskId::from(task_id),
        },
        method: "permission/request".to_string(),
        title: "Permission needed".to_string(),
        params: json!({ "prompt": "Allow?" }),
    }
}

fn task_secret_request(task_id: &str) -> ServerRequestDraft {
    ServerRequestDraft {
        scope: PendingRequestScope::Task {
            task_id: TaskId::from(task_id),
        },
        method: "secret/read".to_string(),
        title: "Secret needed".to_string(),
        params: json!({ "key": "agent.secret" }),
    }
}

fn client_server_request(client_id: &str) -> ServerRequestDraft {
    ServerRequestDraft {
        scope: PendingRequestScope::Client {
            client_instance_id: ClientInstanceId::from(client_id),
        },
        method: "shell/readSecret".to_string(),
        title: "Secret needed".to_string(),
        params: json!({ "key": "agent.secret" }),
    }
}
