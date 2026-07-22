use super::*;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::ProtocolErrorCode;
use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::methods::{
    AGENT_CREATE_CUSTOM, AGENT_DELETE_CUSTOM, AGENT_LIST_SESSIONS, AGENT_PROBE,
    AGENT_REPLACE_CUSTOM, AGENT_SET_ENABLED, AGENT_UPDATE_CUSTOM_METADATA,
    ATTACHMENT_CONFIRM_EMBEDDED, ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
    ATTACHMENT_CREATE_FILE_REFERENCE, ATTACHMENT_CREATE_PASTED_IMAGE, ATTACHMENT_LIST_DIRECTORY,
    ATTACHMENT_LIST_ROOTS, ATTACHMENT_REFRESH_HANDLES, ATTACHMENT_RELEASE, ATTACHMENT_REVEAL,
    CLIENT_HEARTBEAT, CLIENT_INITIALIZE, SETTINGS_GET_AGENT_DETAILS, STATE_SUBSCRIBE, TASK_ACQUIRE,
    TASK_ADOPT_NATIVE_SESSION, TASK_CANCEL, TASK_LIST, TASK_MARK_READ, TASK_OPEN, TASK_RELEASE,
    TASK_SEND, TASK_SET_ARCHIVED, TASK_SET_CONFIG_OPTION,
};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use openaide_app_server_protocol::state::{StateSubscribeParams, SubscriptionScope};

use crate::projects::{project_id_for_workspace, ConfiguredProjectRoots};
use crate::protocol::model::{
    ActivityStep, IsolationKind, NormalizedMessage, TaskStatus, ToolPermissionDecision,
};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;
use crate::task_events::TaskUpdate;

fn dispatcher() -> (tempfile::TempDir, ProtocolEdgeStdioDispatcher) {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    (temp, ProtocolEdgeStdioDispatcher::new_for_test(state_root))
}

fn init_request(id: &str, client_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": CLIENT_INITIALIZE,
        "params": {
            "clientInstanceId": client_id,
            "shell": { "kind": "web" },
            "requestedSurface": { "kind": "home" },
            "capabilities": {
                "protocol": ["permissionResponses", "questionResponses"]
            }
        },
        "meta": { "clientRequestId": "client-request-1" }
    })
    .to_string()
}

fn initialize_params(client_id: &str) -> Value {
    json!({
        "clientInstanceId": client_id,
        "shell": { "kind": "web" },
        "requestedSurface": { "kind": "home" },
        "capabilities": {
            "protocol": ["permissionResponses", "questionResponses"]
        },
    })
}

fn gateway_request(id: &str, method: &str, params: Value) -> InboundProtocolMessage {
    InboundProtocolMessage::ClientRequest {
        id: id.to_string(),
        method: method.to_string(),
        params,
        meta: RequestMeta::default(),
    }
}

fn gateway_result(outcome: GatewayOutcome) -> Value {
    match outcome {
        GatewayOutcome::Respond {
            response: crate::protocol_edge::GatewayResponse::Result(value),
            ..
        } => value["result"].clone(),
        other => panic!("expected gateway result, got {other:?}"),
    }
}

fn gateway_error(
    outcome: GatewayOutcome,
) -> openaide_app_server_protocol::envelopes::ErrorEnvelope {
    match outcome {
        GatewayOutcome::Respond {
            response: crate::protocol_edge::GatewayResponse::Error(error),
            ..
        } => *error,
        other => panic!("expected gateway error, got {other:?}"),
    }
}

fn create_pasted_image_handle(
    gateway: &crate::protocol_edge::SharedRpcGateway,
    connection_id: &ConnectionId,
    request_id: &str,
    now: u64,
) -> String {
    let created = gateway_result(gateway.handle_inbound(
        connection_id.clone(),
        gateway_request(
            request_id,
            ATTACHMENT_CREATE_PASTED_IMAGE,
            json!({
                "taskId": "task-1",
                "label": "image.png",
                "mimeType": "image/png",
                "data": "aW1hZ2U=",
            }),
        ),
        AppServerTime(now),
    ));
    created["attachment"]["handleId"]
        .as_str()
        .expect("attachment handle")
        .to_string()
}

fn response(line: &str) -> Value {
    serde_json::from_str(line).expect("json response")
}

#[test]
fn initialize_succeeds_through_protocol_edge_stdio() {
    let (_temp, mut dispatcher) = dispatcher();

    let responses = dispatcher.handle_line(&init_request("1", "client-1"));

    assert_eq!(responses.len(), 1);
    let response = response(&responses[0]);
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], "1");
    assert_eq!(
        response["result"]["meta"]["clientRequestId"],
        "client-request-1"
    );
    assert_eq!(
        response["result"]["result"]["snapshot"]["client"]["clientInstanceId"],
        "client-1"
    );
    assert_eq!(
        response["result"]["result"]["snapshot"]["newTaskDefaults"],
        serde_json::json!({})
    );
    assert_eq!(
        response["result"]["result"]["snapshot"]["agents"]["agents"][0]["agentId"],
        "codex"
    );
    assert_eq!(
        response["result"]["result"]["snapshot"]["agents"]["agents"][1]["agentId"],
        "opencode"
    );
    assert_eq!(
        response["result"]["result"]["snapshot"]["settings"]["runtime"]["developer"]["acpTrace"]
            ["enabled"],
        false
    );
    assert!(
        response["result"]["result"]["snapshot"]["settings"]["runtime"]["developer"]["acpTrace"]
            ["directory"]
            .as_str()
            .expect("ACP trace directory")
            .contains("diagnostics/acp-traces")
    );
}

#[test]
fn initialize_exposes_one_stable_server_id_per_gateway_process_epoch() {
    let (_first_temp, mut first_process) = dispatcher();
    let first_snapshot = first_process.handle_line(&init_request("first-1", "client-1"));
    let repeated_snapshot = first_process.handle_line(&init_request("first-2", "client-1"));
    let (_second_temp, mut second_process) = dispatcher();
    let second_snapshot = second_process.handle_line(&init_request("second-1", "client-2"));

    let first_server_id = response(&first_snapshot[0])["result"]["result"]["snapshot"]["server"]
        ["serverId"]
        .as_str()
        .expect("first process ServerId")
        .to_string();
    let repeated_server_id = response(&repeated_snapshot[0])["result"]["result"]["snapshot"]
        ["server"]["serverId"]
        .as_str()
        .expect("repeated first process ServerId")
        .to_string();
    let second_server_id = response(&second_snapshot[0])["result"]["result"]["snapshot"]["server"]
        ["serverId"]
        .as_str()
        .expect("second process ServerId")
        .to_string();

    assert!(!first_server_id.is_empty());
    assert_eq!(repeated_server_id, first_server_id);
    assert!(!second_server_id.is_empty());
    assert_ne!(second_server_id, first_server_id);
}

#[test]
fn attachment_browser_resources_and_candidates_are_scoped_to_originating_client() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    std::fs::write(temp.path().join("notes.md"), "private notes").unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.workspace_root = temp.path().to_string_lossy().to_string();
    store.write_task(&task).unwrap();
    drop(store);
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let gateway = dispatcher.shared_gateway();
    let owner_connection = ConnectionId::new("conn-owner");
    let other_connection = ConnectionId::new("conn-other");
    gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "initialize-owner",
            CLIENT_INITIALIZE,
            initialize_params("client-owner"),
        ),
        AppServerTime(1),
    ));
    gateway_result(gateway.handle_inbound(
        other_connection.clone(),
        gateway_request(
            "initialize-other",
            CLIENT_INITIALIZE,
            initialize_params("client-other"),
        ),
        AppServerTime(2),
    ));

    let roots = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "roots",
            ATTACHMENT_LIST_ROOTS,
            json!({ "taskId": "task-1" }),
        ),
        AppServerTime(3),
    ));
    let root_id = roots["roots"][0]["rootId"].as_str().unwrap();
    let listing = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "listing",
            ATTACHMENT_LIST_DIRECTORY,
            json!({ "taskId": "task-1", "rootId": root_id }),
        ),
        AppServerTime(4),
    ));
    let entry_id = listing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["label"] == "notes.md")
        .unwrap()["entryId"]
        .as_str()
        .unwrap()
        .to_string();

    let create_error = gateway_error(gateway.handle_inbound(
        other_connection.clone(),
        gateway_request(
            "create-other",
            ATTACHMENT_CREATE_FILE_REFERENCE,
            json!({ "taskId": "task-1", "entryId": entry_id }),
        ),
        AppServerTime(5),
    ));
    assert_eq!(create_error.error.code, ProtocolErrorCode::ValidationFailed);

    let candidate = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "candidate-owner",
            ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
            json!({ "taskId": "task-1", "entryId": entry_id }),
        ),
        AppServerTime(6),
    ));
    let candidate_id = candidate["candidate"]["candidateId"]
        .as_str()
        .unwrap()
        .to_string();
    let other_confirmation = gateway_result(gateway.handle_inbound(
        other_connection.clone(),
        gateway_request(
            "confirm-other",
            ATTACHMENT_CONFIRM_EMBEDDED,
            json!({ "taskId": "task-1", "candidates": [candidate_id] }),
        ),
        AppServerTime(7),
    ));
    assert_eq!(other_confirmation["attachments"], json!([]));
    assert_eq!(
        other_confirmation["errors"][0]["code"],
        json!("unknownCandidate")
    );

    let denied_release = gateway_result(gateway.handle_inbound(
        other_connection.clone(),
        gateway_request(
            "release-candidate-other",
            ATTACHMENT_RELEASE,
            json!({
                "taskId": "task-1",
                "resources": [{ "kind": "candidate", "id": candidate_id }]
            }),
        ),
        AppServerTime(8),
    ));
    assert_eq!(denied_release["outcomes"][0]["status"], json!("forbidden"));

    let owner_release = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "release-candidate-owner",
            ATTACHMENT_RELEASE,
            json!({
                "taskId": "task-1",
                "resources": [{ "kind": "candidate", "id": candidate_id }]
            }),
        ),
        AppServerTime(9),
    ));
    assert_eq!(owner_release["outcomes"][0]["status"], json!("released"));

    let repeated_release = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "release-candidate-again",
            ATTACHMENT_RELEASE,
            json!({
                "taskId": "task-1",
                "resources": [{ "kind": "candidate", "id": candidate_id }]
            }),
        ),
        AppServerTime(10),
    ));
    assert_eq!(repeated_release["outcomes"][0]["status"], json!("noOp"));

    let replacement_candidate = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "candidate-owner-replacement",
            ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
            json!({ "taskId": "task-1", "entryId": entry_id }),
        ),
        AppServerTime(11),
    ));
    let replacement_candidate_id = replacement_candidate["candidate"]["candidateId"]
        .as_str()
        .unwrap()
        .to_string();

    let owner_confirmation = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "confirm-owner",
            ATTACHMENT_CONFIRM_EMBEDDED,
            json!({ "taskId": "task-1", "candidates": [replacement_candidate_id] }),
        ),
        AppServerTime(12),
    ));
    assert_eq!(
        owner_confirmation["attachments"].as_array().unwrap().len(),
        1
    );

    let file_reference = gateway_result(gateway.handle_inbound(
        owner_connection,
        gateway_request(
            "file-reference-owner",
            ATTACHMENT_CREATE_FILE_REFERENCE,
            json!({ "taskId": "task-1", "entryId": entry_id }),
        ),
        AppServerTime(13),
    ));
    let file_handle = file_reference["attachment"]["handleId"].as_str().unwrap();
    let reveal_error = gateway_error(gateway.handle_inbound(
        other_connection,
        gateway_request(
            "reveal-other",
            ATTACHMENT_REVEAL,
            json!({ "taskId": "task-1", "handleId": file_handle }),
        ),
        AppServerTime(10),
    ));
    assert_eq!(reveal_error.error.code, ProtocolErrorCode::ValidationFailed);
}

#[test]
fn attachment_lease_follows_owner_heartbeat_and_silent_client_expiry() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.workspace_root = temp.path().to_string_lossy().to_string();
    store.write_task(&task).unwrap();
    drop(store);
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let attachment_runtime = dispatcher.attachment_runtime_for_test();
    let gateway = dispatcher.shared_gateway();
    let live_connection = ConnectionId::new("conn-live");
    let silent_connection = ConnectionId::new("conn-silent");
    gateway_result(gateway.handle_inbound(
        live_connection.clone(),
        gateway_request(
            "initialize-live",
            CLIENT_INITIALIZE,
            initialize_params("client-live"),
        ),
        AppServerTime(1),
    ));
    gateway_result(gateway.handle_inbound(
        silent_connection.clone(),
        gateway_request(
            "initialize-silent",
            CLIENT_INITIALIZE,
            initialize_params("client-silent"),
        ),
        AppServerTime(2),
    ));
    let live_handle = create_pasted_image_handle(&gateway, &live_connection, "live-create", 3);
    let silent_handle =
        create_pasted_image_handle(&gateway, &silent_connection, "silent-create", 4);

    attachment_runtime.expire_all_for_test();
    gateway_result(gateway.handle_inbound(
        live_connection.clone(),
        gateway_request("heartbeat", CLIENT_HEARTBEAT, json!({})),
        AppServerTime(5),
    ));
    let live_refresh = gateway_result(gateway.handle_inbound(
        live_connection,
        gateway_request(
            "refresh-live",
            ATTACHMENT_REFRESH_HANDLES,
            json!({ "taskId": "task-1", "handles": [live_handle] }),
        ),
        AppServerTime(6),
    ));
    assert_eq!(live_refresh["attachments"][0]["handleId"], live_handle);

    let silent_refresh = gateway_error(gateway.handle_inbound(
        silent_connection,
        gateway_request(
            "refresh-silent",
            ATTACHMENT_REFRESH_HANDLES,
            json!({ "taskId": "task-1", "handles": [silent_handle] }),
        ),
        AppServerTime(7),
    ));
    assert_eq!(
        silent_refresh.error.code,
        ProtocolErrorCode::ValidationFailed
    );
}

#[test]
fn new_client_lifecycle_cannot_revive_abandoned_attachment_handles() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let mut task = task_record("task-1");
    task.workspace_root = temp.path().to_string_lossy().to_string();
    store.write_task(&task).unwrap();
    drop(store);
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let gateway = dispatcher.shared_gateway();
    let first_connection = ConnectionId::new("conn-first");
    gateway_result(gateway.handle_inbound(
        first_connection.clone(),
        gateway_request(
            "initialize-first",
            CLIENT_INITIALIZE,
            initialize_params("client-reused"),
        ),
        AppServerTime(1),
    ));
    let handle_id = create_pasted_image_handle(&gateway, &first_connection, "create", 2);
    assert!(gateway
        .expire_inactive_clients(AppServerTime(30_002))
        .is_empty());
    let expired = gateway.expire_inactive_clients(AppServerTime(40_002));
    assert_eq!(expired.len(), 1);

    let replacement_connection = ConnectionId::new("conn-replacement");
    gateway_result(gateway.handle_inbound(
        replacement_connection.clone(),
        gateway_request(
            "initialize-replacement",
            CLIENT_INITIALIZE,
            initialize_params("client-reused"),
        ),
        AppServerTime(40_003),
    ));
    let refresh_error = gateway_error(gateway.handle_inbound(
        replacement_connection,
        gateway_request(
            "refresh",
            ATTACHMENT_REFRESH_HANDLES,
            json!({ "taskId": "task-1", "handles": [handle_id] }),
        ),
        AppServerTime(40_004),
    ));
    assert_eq!(
        refresh_error.error.code,
        ProtocolErrorCode::ValidationFailed
    );
}

#[test]
fn initialize_uses_stored_agent_catalog_in_production_startup_path() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    std::fs::create_dir_all(temp.path().join("agents")).unwrap();
    std::fs::write(
        temp.path().join("agents").join("catalog.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "records": [
                {
                    "id": "codex",
                    "enabled": false
                },
                {
                    "id": "custom.local",
                    "label": "Local Agent",
                    "source_kind": "custom",
                    "transport": "stdio",
                    "command": "local-agent"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::try_new(state_root).unwrap();

    let responses = dispatcher.handle_line(&init_request("1", "client-1"));

    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["snapshot"]["agents"]["agents"][0]["agentId"],
        "custom.local"
    );
    assert_eq!(
        response["result"]["result"]["snapshot"]["agents"]["agents"][1]["agentId"],
        "opencode"
    );
}

#[test]
fn local_http_factory_disables_acp_host_requests_until_a_typed_responder_exists() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::try_new_with_host_request_transport(
        state_root,
        AcpHostRequestTransport::Unavailable,
    )
    .unwrap();

    assert!(!dispatcher.host_bridge.is_enabled());
    assert!(dispatcher.take_host_requests().is_none());
}

#[test]
fn plain_stdio_factory_preserves_acp_host_request_transport() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::try_new(state_root).unwrap();

    assert!(dispatcher.host_bridge.is_enabled());
    assert!(dispatcher.take_host_requests().is_some());
}

#[test]
fn initialize_returns_storage_backed_task_navigation() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-1")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);

    let responses = dispatcher.handle_line(&init_request("1", "client-1"));

    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["snapshot"]["tasks"]["entries"][0]["task"]["taskId"],
        "task-1"
    );
}

#[test]
fn agent_probe_updates_agent_snapshot_and_emits_event() {
    let (_temp, mut dispatcher) = dispatcher();
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::Agents,
            }
        })
        .to_string(),
    );

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": AGENT_PROBE,
            "params": { "agentId": "codex" }
        })
        .to_string(),
    );

    assert_eq!(responses.len(), 2);
    let probe_response = response(&responses[0]);
    assert_eq!(probe_response["id"], "2");
    assert_eq!(
        probe_response["result"]["result"]["agents"]["agents"][0]["agentId"],
        "codex"
    );
    assert_eq!(
        probe_response["result"]["result"]["agents"]["agents"][0]["status"],
        "connected"
    );

    let event = response(&responses[1]);
    assert_eq!(event["method"], "app/event");
    assert_eq!(event["params"]["payload"]["kind"], "agentCollectionUpdated");
    assert_eq!(
        event["params"]["payload"]["agents"]["agents"][0]["status"],
        "connected"
    );
}

#[test]
fn agent_custom_create_updates_live_registry_and_emits_agent_event() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let custom_agent_id: String;
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut project_anchor = task_record("task-existing");
        project_anchor.lifecycle = crate::storage::records::TaskLifecycle::Visible;
        store.write_task(&project_anchor).unwrap();
    }
    {
        let state_root = StateRoot::resolve(temp.path()).expect("state root");
        let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
        dispatcher.handle_line(&init_request("1", "client-1"));
        dispatcher.handle_line(
            &json!({
                "jsonrpc": "2.0",
                "id": "subscribe",
                "method": STATE_SUBSCRIBE,
                "params": StateSubscribeParams {
                    scope: SubscriptionScope::Agents,
                }
            })
            .to_string(),
        );

        let responses = dispatcher.handle_line(
            &json!({
                "jsonrpc": "2.0",
                "id": "create-agent",
                "method": AGENT_CREATE_CUSTOM,
                "params": {
                    "label": "Local Agent",
                    "icon": "terminal",
                    "commandLine": "local-agent \"acp mode\"",
                    "command": "local-agent",
                    "args": ["acp mode"],
                    "secretEnv": ["LOCAL_TOKEN"]
                }
            })
            .to_string(),
        );

        assert_eq!(responses.len(), 2);
        let create_response = response(&responses[0]);
        assert_eq!(create_response["id"], "create-agent");
        custom_agent_id = create_response["result"]["result"]["agentId"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(custom_agent_id.starts_with("custom."));
        assert_eq!(
            create_response["result"]["result"]["agents"]["agents"][0]["agentId"],
            "codex"
        );
        assert!(create_response["result"]["result"]["agents"]["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|agent| agent["agentId"] == custom_agent_id));

        let event = response(&responses[1]);
        assert_eq!(event["method"], "app/event");
        assert_eq!(event["params"]["payload"]["kind"], "agentCollectionUpdated");
        assert!(event["params"]["payload"]["agents"]["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|agent| agent["agentId"] == custom_agent_id));

        let details = dispatcher.handle_line(
            &json!({
                "jsonrpc": "2.0",
                "id": "agent-details",
                "method": SETTINGS_GET_AGENT_DETAILS,
                "params": {}
            })
            .to_string(),
        );
        assert_eq!(details.len(), 1);
        let details_response = response(&details[0]);
        let custom = details_response["result"]["result"]["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["agentId"] == custom_agent_id)
            .unwrap();
        assert_eq!(custom["icon"], "terminal");
        assert_eq!(custom["commandLine"], "local-agent \"acp mode\"");
        assert_eq!(custom["env"][0]["name"], "LOCAL_TOKEN");
    }

    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut restarted = ProtocolEdgeStdioDispatcher::try_new(state_root).unwrap();
    let responses = restarted.handle_line(&init_request("restart", "client-2"));
    let restart_response = response(&responses[0]);
    assert!(
        restart_response["result"]["result"]["snapshot"]["agents"]["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|agent| agent["agentId"] == custom_agent_id)
    );
    let create_responses = restarted.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "create-custom-task",
            "method": TASK_ACQUIRE,
            "params": {
                "agentId": custom_agent_id,
                "projectId": project_id_for_workspace("/tmp/openaide-stdio-workspace/a"),
                "title": "Custom task"
            }
        })
        .to_string(),
    );
    let create_response = response(&create_responses[0]);
    assert_eq!(create_response["id"], "create-custom-task");
    assert_eq!(
        create_response["result"]["result"]["task"]["task"]["agentId"],
        custom_agent_id
    );
}

#[test]
fn agent_enabled_and_delete_mutations_update_catalog_snapshot() {
    let (_temp, mut dispatcher) = dispatcher();
    dispatcher.handle_line(&init_request("1", "client-1"));
    let created = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "create-agent",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "Local Agent",
                "command": "local-agent"
            }
        })
        .to_string(),
    );
    let custom_agent_id = response(&created[0])["result"]["result"]["agentId"]
        .as_str()
        .unwrap()
        .to_string();

    let disabled = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "disable-codex",
            "method": AGENT_SET_ENABLED,
            "params": {
                "agentId": "codex",
                "enabled": false
            }
        })
        .to_string(),
    );

    let disabled_response = response(&disabled[0]);
    assert!(disabled_response["result"]["result"]["agents"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .all(|agent| agent["agentId"] != "codex"));

    let deleted = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "delete-custom",
            "method": AGENT_DELETE_CUSTOM,
            "params": {
                "agentId": custom_agent_id
            }
        })
        .to_string(),
    );

    let delete_response = response(&deleted[0]);
    assert_eq!(
        delete_response["result"]["result"]["agentId"],
        custom_agent_id
    );
    assert!(delete_response["result"]["result"]["agents"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .all(|agent| agent["agentId"] != custom_agent_id));
}

#[test]
fn agent_custom_update_and_replace_use_distinct_identity_rules() {
    let (_temp, mut dispatcher) = dispatcher();
    dispatcher.handle_line(&init_request("1", "client-1"));
    let created = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "create-agent",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "Local Agent",
                "icon": "bot",
                "commandLine": "local-agent",
                "command": "local-agent"
            }
        })
        .to_string(),
    );
    let old_agent_id = response(&created[0])["result"]["result"]["agentId"]
        .as_str()
        .unwrap()
        .to_string();

    let updated = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "update-metadata",
            "method": AGENT_UPDATE_CUSTOM_METADATA,
            "params": {
                "agentId": old_agent_id,
                "label": "Renamed Agent",
                "icon": "terminal",
                "enabled": false
            }
        })
        .to_string(),
    );
    let update_response = response(&updated[0]);
    assert_eq!(update_response["result"]["result"]["agentId"], old_agent_id);

    let metadata_only_replace = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "replace-metadata-only",
            "method": AGENT_REPLACE_CUSTOM,
            "params": {
                "sourceAgentId": old_agent_id,
                "label": "Metadata-only Replacement",
                "icon": "terminal",
                "commandLine": "local-agent",
                "command": "local-agent",
                "enabled": true,
                "confirmation": {
                    "acceptedLaunchIdentityChange": true
                }
            }
        })
        .to_string(),
    );
    assert_eq!(
        response(&metadata_only_replace[0])["error"]["error"]["code"],
        "validationFailed"
    );

    let missing_confirmation = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "replace-without-confirmation",
            "method": AGENT_REPLACE_CUSTOM,
            "params": {
                "sourceAgentId": old_agent_id,
                "label": "Replacement Agent",
                "icon": "terminal",
                "commandLine": "replacement-agent",
                "command": "replacement-agent",
                "enabled": true,
                "confirmation": {
                    "acceptedLaunchIdentityChange": false
                }
            }
        })
        .to_string(),
    );
    assert_eq!(
        response(&missing_confirmation[0])["error"]["error"]["code"],
        "validationFailed"
    );

    let replaced = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "replace-agent",
            "method": AGENT_REPLACE_CUSTOM,
            "params": {
                "sourceAgentId": old_agent_id,
                "label": "Replacement Agent",
                "icon": "terminal",
                "commandLine": "replacement-agent",
                "command": "replacement-agent",
                "enabled": true,
                "confirmation": {
                    "acceptedLaunchIdentityChange": true
                }
            }
        })
        .to_string(),
    );
    let replace_response = response(&replaced[0]);
    let new_agent_id = replace_response["result"]["result"]["newAgentId"]
        .as_str()
        .unwrap();
    assert_eq!(
        replace_response["result"]["result"]["oldAgentId"],
        old_agent_id
    );
    assert_eq!(
        replace_response["result"]["result"]["cleanup"]["historyPolicy"],
        "preserveHistoricalTasks"
    );
    assert_eq!(
        replace_response["result"]["result"]["cleanup"]["removedCatalogRecord"],
        true
    );
    assert_ne!(new_agent_id, old_agent_id);
    assert!(replace_response["result"]["result"]["agents"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .all(|agent| agent["agentId"] != old_agent_id));
    assert!(replace_response["result"]["result"]["agents"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|agent| agent["agentId"] == new_agent_id));
}

#[test]
fn rejected_agent_mutations_return_errors_without_agent_events() {
    let (_temp, mut dispatcher) = dispatcher();
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::Agents,
            }
        })
        .to_string(),
    );

    let invalid_create = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "invalid-create",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "",
                "command": "codex"
            }
        })
        .to_string(),
    );
    assert_eq!(invalid_create.len(), 1);
    let invalid_save_response = response(&invalid_create[0]);
    assert_eq!(
        invalid_save_response["error"]["error"]["code"],
        "validationFailed"
    );

    let duplicate_builtin_label = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "duplicate-builtin-label",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": " codex ",
                "command": "local-agent"
            }
        })
        .to_string(),
    );
    assert_eq!(duplicate_builtin_label.len(), 1);
    let duplicate_builtin_label_response = response(&duplicate_builtin_label[0]);
    assert_eq!(
        duplicate_builtin_label_response["error"]["error"]["code"],
        "validationFailed"
    );

    let created = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "create-agent",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "Local Agent",
                "command": "local-agent"
            }
        })
        .to_string(),
    );
    let custom_agent_id = response(&created[0])["result"]["result"]["agentId"]
        .as_str()
        .unwrap()
        .to_string();

    let duplicate_custom_label = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "duplicate-custom-label",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "Local Agent",
                "command": "other-agent"
            }
        })
        .to_string(),
    );
    assert_eq!(duplicate_custom_label.len(), 1);
    let duplicate_custom_label_response = response(&duplicate_custom_label[0]);
    assert_eq!(
        duplicate_custom_label_response["error"]["error"]["code"],
        "validationFailed"
    );

    let duplicate_command = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "duplicate-command",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "Duplicate Command",
                "command": " local-agent "
            }
        })
        .to_string(),
    );
    assert_eq!(duplicate_command.len(), 1);
    let duplicate_command_response = response(&duplicate_command[0]);
    assert_eq!(
        duplicate_command_response["error"]["error"]["code"],
        "validationFailed"
    );

    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "disable-custom",
            "method": AGENT_SET_ENABLED,
            "params": {
                "agentId": custom_agent_id,
                "enabled": false
            }
        })
        .to_string(),
    );

    let duplicate_disabled_command = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "duplicate-disabled-command",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "Duplicate Disabled Command",
                "command": "local-agent"
            }
        })
        .to_string(),
    );
    assert_eq!(duplicate_disabled_command.len(), 1);
    let duplicate_disabled_command_response = response(&duplicate_disabled_command[0]);
    assert_eq!(
        duplicate_disabled_command_response["error"]["error"]["code"],
        "validationFailed"
    );

    let create_disabled = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "create-disabled",
            "method": AGENT_CREATE_CUSTOM,
            "params": {
                "label": "Disabled Agent",
                "command": "disabled-agent",
                "enabled": false
            }
        })
        .to_string(),
    );
    let create_disabled_response = response(&create_disabled[0]);
    assert_eq!(create_disabled_response["id"], "create-disabled");
    assert!(create_disabled_response["result"]["result"]["agentId"]
        .as_str()
        .unwrap()
        .starts_with("custom."));

    let missing_delete = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "missing-delete",
            "method": AGENT_DELETE_CUSTOM,
            "params": {
                "agentId": "custom.missing"
            }
        })
        .to_string(),
    );
    assert_eq!(missing_delete.len(), 1);
    let missing_delete_response = response(&missing_delete[0]);
    assert_eq!(
        missing_delete_response["error"]["error"]["code"],
        "capabilityUnavailable"
    );

    let unknown_enable = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "unknown-enable",
            "method": AGENT_SET_ENABLED,
            "params": {
                "agentId": "custom.unknown",
                "enabled": false
            }
        })
        .to_string(),
    );
    assert_eq!(unknown_enable.len(), 1);
    let unknown_enable_response = response(&unknown_enable[0]);
    assert_eq!(
        unknown_enable_response["error"]["error"]["code"],
        "capabilityUnavailable"
    );
}

#[test]
fn dispatcher_startup_isolates_damaged_task_storage() {
    let (temp, dispatcher) = dispatcher();
    drop(dispatcher);
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("corrupt-task")).unwrap();
    }
    corrupt_last_byte(
        &temp
            .path()
            .join("task-store-v1/tasks/corrupt-task/task.journal"),
    );
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));
    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_LIST,
            "params": {}
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["result"]["result"]["tasks"], json!([]));
}

#[test]
fn task_list_returns_storage_backed_tasks_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-1")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_LIST,
            "params": {}
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["result"]["result"]["tasks"][0]["taskId"], "task-1");
    assert_eq!(response["result"]["result"]["revision"], 1);
}

#[test]
fn task_open_returns_storage_backed_task_snapshot_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut task = task_record("task-1");
        task.unread = true;
        store.write_task(&task).unwrap();
        store
            .append_message(
                "task-1",
                crate::protocol::model::ChatMessage {
                    cursor: "cursor-1".to_string(),
                    identity: "user-1".to_string(),
                    message_type: "user".to_string(),
                    message_id: "user-1".to_string(),
                    message: crate::protocol::model::NormalizedMessage::User {
                        id: "user-1".to_string(),
                        text: "hello".to_string(),
                        created_at: "2026-01-01T00:00:00.000Z".to_string(),
                        attachments: Vec::new(),
                    },
                },
            )
            .unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_OPEN,
            "params": { "taskId": "task-1" }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["task"]["task"]["taskId"],
        "task-1"
    );
    assert_eq!(
        response["result"]["result"]["task"]["task"]["unread"],
        false
    );
    assert_eq!(
        response["result"]["result"]["task"]["chat"]["items"][0]["parts"][0]["text"],
        "hello"
    );
    drop(dispatcher);
    assert!(
        !Store::open(temp.path().to_path_buf())
            .unwrap()
            .read_task("task-1")
            .unwrap()
            .unread
    );
}

#[test]
fn task_mark_read_acknowledges_unread_task_over_stdio() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut task = task_record("task-1");
        task.unread = true;
        store.write_task(&task).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_MARK_READ,
            "params": { "taskId": "task-1" }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["task"]["task"]["unread"],
        false
    );
    drop(dispatcher);
    assert!(
        !Store::open(temp.path().to_path_buf())
            .unwrap()
            .read_task("task-1")
            .unwrap()
            .unread
    );
}

fn corrupt_last_byte(path: &std::path::Path) {
    use std::io::{Read, Seek, Write};
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.seek(std::io::SeekFrom::End(-1)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    file.seek(std::io::SeekFrom::End(-1)).unwrap();
    file.write_all(&[byte[0] ^ 0xff]).unwrap();
    file.sync_all().unwrap();
}

#[test]
fn task_subscription_emits_pending_server_request_over_stdio() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-1")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));
    let opened = dispatcher.gateway.open_server_request(
        task_secret_request("task-1"),
        crate::client_lifecycle::AppServerTime(2),
    );
    assert!(matches!(
        opened,
        OpenRequestOutcome::Opened {
            deliveries,
            ..
        } if deliveries.is_empty()
    ));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": STATE_SUBSCRIBE,
            "params": {
                "scope": {
                    "kind": "task",
                    "taskId": "task-1"
                }
            }
        })
        .to_string(),
    );

    assert_eq!(responses.len(), 2);
    let subscribe_response = response(&responses[0]);
    assert_eq!(subscribe_response["id"], "2");
    assert_eq!(
        subscribe_response["result"]["result"]["snapshot"]["task"]["task"]["title"],
        serde_json::json!({ "value": "Task", "source": "user" })
    );
    assert_eq!(
        subscribe_response["result"]["result"]["snapshot"]["task"]["pendingRequests"][0]
            ["requestId"],
        "server-request-1"
    );
    let server_request = response(&responses[1]);
    assert_eq!(server_request["jsonrpc"], "2.0");
    assert_eq!(server_request["id"], "server-request-1");
    assert_eq!(server_request["method"], "secret/read");
    assert_eq!(server_request["params"]["key"], "agent.secret");

    let updates = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "result": { "decision": "allow" }
        })
        .to_string(),
    );

    assert_eq!(updates.len(), 1);
    let update = response(&updates[0]);
    assert_eq!(update["method"], "app/event");
    assert_eq!(update["params"]["payload"]["kind"], "taskRequestsUpdated");
    assert!(update["params"]["payload"]["requests"]
        .as_array()
        .is_some_and(Vec::is_empty));
    assert!(dispatcher
        .gateway
        .pending_server_requests_for_task(&TaskId::from("task-1"))
        .is_empty());
    drop(temp);
}

#[test]
fn stdio_error_response_does_not_resolve_pending_server_request() {
    let (temp, mut dispatcher) = dispatcher();
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": STATE_SUBSCRIBE,
            "params": {
                "scope": {
                    "kind": "task",
                    "taskId": "task-1"
                }
            }
        })
        .to_string(),
    );
    let opened = dispatcher
        .gateway
        .open_server_request(task_server_request("task-1"), AppServerTime(2));
    assert!(matches!(
        opened,
        OpenRequestOutcome::Opened {
            deliveries,
            ..
        } if deliveries.len() == 1
    ));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "server-request-1",
            "error": { "code": -32000, "message": "denied" }
        })
        .to_string(),
    );

    assert!(responses.is_empty());
    assert_eq!(
        dispatcher
            .gateway
            .pending_server_requests_for_task(&TaskId::from("task-1"))
            .len(),
        1
    );
    drop(temp);
}

#[test]
fn stdio_client_response_parser_ignores_non_server_request_ids() {
    assert!(client_response(&json!({
        "jsonrpc": "2.0",
        "id": 7,
        "result": {}
    }))
    .is_none());
    assert!(client_response(&json!({
        "jsonrpc": "2.0",
        "id": "host-request-1",
        "result": {}
    }))
    .is_none());
}

#[test]
fn task_create_persists_idle_task_without_prompt_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_ACQUIRE,
            "params": {
                "projectId": project_id_for_workspace("/tmp/openaide-stdio-workspace/a"),
                "agentId": "codex"
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    let task_id = response["result"]["result"]["task"]["task"]["taskId"]
        .as_str()
        .expect("created task id")
        .to_string();
    assert_ne!(task_id, "task-existing");
    assert_eq!(
        response["result"]["result"]["task"]["preparation"]["kind"],
        "preparing"
    );
    assert_eq!(
        response["result"]["result"]["task"]["sendCapability"]["state"],
        "loading"
    );
    assert_eq!(
        response["result"]["result"]["task"]["chat"]["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    let record = store.read_task(&task_id).unwrap();
    assert_eq!(record.status, TaskStatus::Inactive);
    assert_eq!(
        record.lifecycle,
        crate::storage::records::TaskLifecycle::New {
            lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
                "client-1"
            )),
        }
    );
    assert_eq!(record.active_turn_id, None);
}

#[test]
fn task_create_does_not_publish_private_new_task_to_project_subscribers() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let notifications = dispatcher.take_task_updates().expect("task updates");
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe-projects",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams { scope: SubscriptionScope::Projects }
        })
        .to_string(),
    );

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_ACQUIRE,
            "params": {
                "projectId": project_id_for_workspace("/tmp/openaide-stdio-workspace/a"),
                "agentId": "codex"
            }
        })
        .to_string(),
    );

    assert_eq!(responses.len(), 1);
    let committed = notifications
        .recv_timeout(Duration::from_secs(1))
        .expect("task create notification");
    let events = dispatcher.handle_task_update(committed);
    assert!(events.is_empty());
}

#[test]
fn task_adopt_native_session_loads_agent_session_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "list",
            "method": AGENT_LIST_SESSIONS,
            "params": {
                "projectId": project_id_for_workspace("/tmp/openaide-stdio-workspace/a"),
                "agentId": "codex",
                "cursor": null
            }
        })
        .to_string(),
    );

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "adopt",
            "method": TASK_ADOPT_NATIVE_SESSION,
            "params": {
                "agentId": "codex",
                "nativeSessionId": "mock-session"
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    let task_id = response["result"]["result"]["task"]["task"]["taskId"]
        .as_str()
        .expect("adopted task id")
        .to_string();
    assert!(task_id.starts_with("task_"));
    assert_eq!(
        response["result"]["result"]["task"]["task"]["title"],
        serde_json::json!({ "value": "Mock session", "source": "agent" })
    );
    assert_eq!(
        response["result"]["result"]["task"]["chat"]["items"][0]["parts"][0]["text"],
        "Mock loaded session."
    );
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    let record = store.read_task(&task_id).unwrap();
    assert_eq!(record.agent_session_id.as_deref(), Some("mock-session"));
    assert_eq!(
        record.lifecycle,
        crate::storage::records::TaskLifecycle::Visible
    );
    assert!(matches!(record.preparation, TaskPreparationRecord::Ready));
}

#[test]
fn task_adopt_native_session_rejects_an_unknown_catalog_identity() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "adopt-missing",
            "method": TASK_ADOPT_NATIVE_SESSION,
            "params": {
                "agentId": "codex",
                "nativeSessionId": "missing-session"
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["id"], "adopt-missing");
    assert_eq!(response["error"]["error"]["code"], "notFound");
    assert_eq!(
        response["error"]["error"]["message"],
        "Native Session is no longer available"
    );
}

#[test]
fn task_send_commits_user_message_and_active_turn_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let task_id = {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
        "task-existing".to_string()
    };
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let notifications = dispatcher.take_task_updates().expect("task updates");
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation { project_id: None },
            }
        })
        .to_string(),
    );

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_SEND,
            "params": {
                "taskId": task_id,
                "message": { "text": "hello from protocol edge" }
            }
        })
        .to_string(),
    );

    assert_eq!(
        responses.len(),
        1,
        "the mutation response must not duplicate notifier events"
    );
    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["task"]["task"]["status"],
        "starting"
    );
    assert_eq!(
        response["result"]["result"]["task"]["chat"]["items"][0]["parts"][0]["text"],
        "hello from protocol edge"
    );
    assert!(response["result"]["result"]["turnId"]
        .as_str()
        .unwrap()
        .starts_with("turn_"));
    let committed = notifications
        .recv_timeout(Duration::from_secs(1))
        .expect("committed send notification");
    let events = dispatcher.handle_task_update(committed);
    assert!(events
        .iter()
        .any(|line| event_payload_kind(line, "taskNavigationReplaced")));
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    let record = store.read_task("task-existing").unwrap();
    assert_eq!(
        record.lifecycle,
        crate::storage::records::TaskLifecycle::Visible
    );
    assert!(record.agent_session_id.is_some());
    assert!(store.read_messages("task-existing").unwrap().len() >= 2);
}

#[test]
fn runtime_task_update_notification_emits_app_event_after_agent_completion() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let task_id = {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
        "task-existing".to_string()
    };
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let notifications = dispatcher.take_task_updates().expect("task updates");
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-existing"),
                },
            }
        })
        .to_string(),
    );
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_SEND,
            "params": {
                "taskId": task_id,
                "message": { "text": "hello from protocol edge" }
            }
        })
        .to_string(),
    );

    let deadline = Instant::now() + Duration::from_secs(1);
    let mut saw_completion_event = false;
    while Instant::now() < deadline {
        let notification = notifications
            .recv_timeout(Duration::from_millis(50))
            .expect("task update notification");
        let messages = dispatcher.handle_task_update(notification);
        if messages.iter().any(|line| completed_task_event(line)) {
            saw_completion_event = true;
            break;
        }
    }

    assert!(saw_completion_event);
}

#[test]
fn task_update_notification_emits_focused_task_and_navigation_changes() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let record = task_record("task-existing");
    let navigation_task = crate::snapshots::project_task_summary(record.clone());
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&record).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe-task",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::Task { task_id: TaskId::from("task-existing") },
            }
        })
        .to_string(),
    );
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe-navigation",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation { project_id: None },
            }
        })
        .to_string(),
    );

    let messages = dispatcher.handle_task_update(TaskUpdate {
        task_id: "task-existing".to_string(),
        revision: 2,
        kind: crate::task_events::TaskUpdateKind::Changed(Box::new(
            crate::task_events::CommittedTaskChange {
                changes: openaide_app_server_protocol::events::TaskChanges::default(),
                tool_details: Vec::new(),
                navigation: Some(
                    openaide_app_server_protocol::events::TaskNavigationChange::Upsert {
                        task: Box::new(navigation_task),
                    },
                ),
            },
        )),
    });

    assert!(messages
        .iter()
        .any(|line| event_payload_kind(line, "taskNavigationChanged")));
    assert!(!messages
        .iter()
        .any(|line| event_payload_kind(line, "projectCollectionUpdated")));
}

#[test]
fn runtime_permission_request_round_trips_over_server_request_stdio() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let notifications = dispatcher.take_task_updates().expect("task updates");
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-existing"),
                },
            }
        })
        .to_string(),
    );
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_SEND,
            "params": {
                "taskId": "task-existing",
                "message": { "text": "please request permission" }
            }
        })
        .to_string(),
    );

    let request = wait_for_server_request(&mut dispatcher, &notifications, "permission/request");
    assert_eq!(request["id"], "server-request-1");
    assert!(request["params"]["requestId"]
        .as_str()
        .expect("agent permission request id")
        .starts_with("perm_"));
    let option_id = request["params"]["options"][0]["optionId"]
        .as_str()
        .expect("option id")
        .to_string();

    let updates = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": { "optionId": option_id }
        })
        .to_string(),
    );
    assert!(updates.iter().any(|line| {
        let value = response(line);
        value["method"] == "app/event"
            && value["params"]["payload"]["kind"] == "taskRequestsUpdated"
    }));

    wait_for_protocol_task_status(&mut dispatcher, &notifications, "task-existing", "idle");
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    let permission = store
        .read_messages("task-existing")
        .unwrap()
        .into_iter()
        .find_map(|message| match message.chat.message {
            NormalizedMessage::Activity { steps, .. } => steps.into_iter().find_map(|step| {
                let ActivityStep::Tool {
                    permission_outcomes,
                    ..
                } = step
                else {
                    return None;
                };
                permission_outcomes.into_iter().next()
            }),
            _ => None,
        })
        .expect("tool permission outcome");
    assert_eq!(permission.request_id, "server-request-1");
    assert_eq!(permission.option_id, Some(option_id));
    assert_eq!(permission.decision, ToolPermissionDecision::Approved);
}

#[test]
fn runtime_permission_request_reject_option_persists_denied_decision() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    let notifications = dispatcher.take_task_updates().expect("task updates");
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::Task {
                    task_id: TaskId::from("task-existing"),
                },
            }
        })
        .to_string(),
    );
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_SEND,
            "params": {
                "taskId": "task-existing",
                "message": { "text": "please request permission" }
            }
        })
        .to_string(),
    );

    let request = wait_for_server_request(&mut dispatcher, &notifications, "permission/request");
    let option_id = request["params"]["options"][1]["optionId"]
        .as_str()
        .expect("reject option id")
        .to_string();
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": { "optionId": option_id }
        })
        .to_string(),
    );

    wait_for_protocol_task_status(&mut dispatcher, &notifications, "task-existing", "idle");
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    let permission = store
        .read_messages("task-existing")
        .unwrap()
        .into_iter()
        .find_map(|message| match message.chat.message {
            NormalizedMessage::Activity { steps, .. } => steps.into_iter().find_map(|step| {
                let ActivityStep::Tool {
                    permission_outcomes,
                    ..
                } = step
                else {
                    return None;
                };
                permission_outcomes.into_iter().next()
            }),
            _ => None,
        })
        .expect("tool permission outcome");
    assert_eq!(permission.request_id, "server-request-1");
    assert_eq!(permission.option_id, Some(option_id));
    assert_eq!(permission.decision, ToolPermissionDecision::Rejected);
}

#[test]
fn task_cancel_clears_active_turn_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut record = task_record("task-existing");
        record.status = TaskStatus::Active;
        record.active_turn_id = Some("turn-active".to_string());
        store.write_task(&record).unwrap();
        store
            .append_message(
                "task-existing",
                crate::protocol::model::ChatMessage {
                    cursor: "m:1".to_string(),
                    identity: "turn:turn-active".to_string(),
                    message_type: "activity".to_string(),
                    message_id: "message-active".to_string(),
                    message: crate::protocol::model::NormalizedMessage::Activity {
                        id: "turn:turn-active".to_string(),
                        title: "Working".to_string(),
                        status: crate::protocol::model::ActivityStatus::Running,
                        created_at: "2026-01-01T00:00:00.000Z".to_string(),
                        collapsed: true,
                        steps: Vec::new(),
                    },
                },
            )
            .unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_CANCEL,
            "params": {
                "taskId": "task-existing",
                "turnId": "turn-active"
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["task"]["task"]["status"],
        "idle"
    );
    drop(dispatcher);
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    assert_eq!(
        store.read_task("task-existing").unwrap().active_turn_id,
        None
    );
}

#[test]
fn task_set_config_option_rejects_a_task_without_a_live_catalog() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-existing")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_SET_CONFIG_OPTION,
            "params": {
                "taskId": "task-existing",
                "configId": "model",
                "value": { "type": "id", "value": "gpt-5.5" },
                "clientMutationId": "mutation-1"
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert!(response.get("error").is_some());
    drop(dispatcher);
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    assert!(store
        .read_task("task-existing")
        .unwrap()
        .config_options_catalog
        .is_none());
}

#[test]
fn product_request_before_initialize_is_rejected_by_gateway() {
    let (_temp, mut dispatcher) = dispatcher();

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::Projects,
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["error"]["error"]["code"], "notInitialized");
    assert_eq!(
        response["error"]["error"]["message"],
        "client/initialize must succeed before product requests"
    );
}

#[test]
fn unsupported_mutating_task_methods_do_not_fall_through_to_legacy_dispatch() {
    let (_temp, mut dispatcher) = dispatcher();
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "unknown-task-method",
            "method": "task/notImplemented",
            "params": {}
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["error"]["error"]["code"], "invalidRequest");
    assert_eq!(
        response["error"]["error"]["message"],
        "Unsupported method: task/notImplemented"
    );
}

#[test]
fn task_release_is_idempotent_after_restart_clears_the_lease() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut draft = task_record("task-draft");
        draft.lifecycle = crate::storage::records::TaskLifecycle::New {
            lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
                "client-1",
            )),
        };
        store.write_task(&draft).unwrap();
        let mut existing = task_record("task-existing");
        existing.lifecycle = crate::storage::records::TaskLifecycle::Visible;
        store.write_task(&existing).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams {
                scope: SubscriptionScope::TaskNavigation { project_id: None },
            }
        })
        .to_string(),
    );

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_RELEASE,
            "params": { "taskId": "task-draft" }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["result"]["result"]["taskId"], "task-draft");
    assert_eq!(responses.len(), 1);
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    assert_eq!(
        store.read_task("task-draft").unwrap().lifecycle,
        crate::storage::records::TaskLifecycle::New { lease: None }
    );
}

#[test]
fn task_set_archived_moves_task_between_navigation_lists() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-active")).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "archive",
            "method": TASK_SET_ARCHIVED,
            "params": { "taskId": "task-active", "archived": true }
        })
        .to_string(),
    );
    let archive_response = response(&responses[0]);
    assert_eq!(
        archive_response["result"]["result"]["taskId"],
        "task-active"
    );
    assert_eq!(archive_response["result"]["result"]["archived"], true);
    assert!(archive_response["result"]["result"].get("tasks").is_none());

    let active = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "list-active",
            "method": TASK_LIST,
            "params": { "archived": false }
        })
        .to_string(),
    );
    assert!(response(&active[0])["result"]["result"]["tasks"]
        .as_array()
        .expect("active tasks after archive")
        .is_empty());

    let archived = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "list-archived",
            "method": TASK_LIST,
            "params": { "archived": true }
        })
        .to_string(),
    );
    let archived_response = response(&archived[0]);
    let tasks = archived_response["result"]["result"]["tasks"]
        .as_array()
        .expect("archived tasks");
    assert!(tasks.iter().any(|task| task["taskId"] == "task-active"));
}

#[test]
fn task_discard_keeps_the_configured_project_after_its_last_task() {
    let workspace_root = "/tmp/openaide-stdio-workspace/configured-project";
    std::fs::create_dir_all(workspace_root).unwrap();
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut draft = task_record("task-draft");
        draft.workspace_root = workspace_root.to_string();
        draft.lifecycle = crate::storage::records::TaskLifecycle::New {
            lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
                "client-1",
            )),
        };
        store.write_task(&draft).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let configured_projects =
        ConfiguredProjectRoots::from_workspace_roots([workspace_root.to_string()]);
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test_with_configured_projects(
        state_root,
        configured_projects,
    );
    let notifications = dispatcher.take_task_updates().expect("task updates");
    dispatcher.handle_line(&init_request("1", "client-1"));
    let subscription = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "subscribe-projects",
            "method": STATE_SUBSCRIBE,
            "params": StateSubscribeParams { scope: SubscriptionScope::Projects }
        })
        .to_string(),
    );
    let project_snapshot = response(&subscription[0]);
    assert_eq!(
        project_snapshot["result"]["result"]["snapshot"]["projects"]["projects"],
        json!([{
            "projectId": project_id_for_workspace(workspace_root),
            "label": "configured-project",
            "workspaceRoot": workspace_root,
            "available": true,
        }])
    );

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_RELEASE,
            "params": { "taskId": "task-draft" }
        })
        .to_string(),
    );

    assert_eq!(responses.len(), 1);
    let committed = notifications
        .recv_timeout(Duration::from_secs(1))
        .expect("task discard notification");
    assert!(dispatcher.handle_task_update(committed).is_empty());
}

#[test]
fn task_open_rejects_discarded_task_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut record = task_record("task-draft");
        record.tombstoned = true;
        store.write_task(&record).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "2",
            "method": TASK_OPEN,
            "params": { "taskId": "task-draft" }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["error"]["error"]["code"], "notFound");
}

#[test]
fn invalid_json_returns_protocol_edge_error_with_null_id() {
    let (_temp, mut dispatcher) = dispatcher();

    let responses = dispatcher.handle_line("{not-json");

    let response = response(&responses[0]);
    assert_eq!(response["jsonrpc"], "2.0");
    assert!(response["id"].is_null());
    assert_eq!(response["error"]["error"]["code"], "invalidRequest");
    assert!(response["error"]["error"]["message"]
        .as_str()
        .expect("message")
        .starts_with("Parse error:"));
}

#[test]
fn invalid_jsonrpc_version_returns_protocol_edge_error() {
    let (_temp, mut dispatcher) = dispatcher();

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "1.0",
            "id": "bad-version",
            "method": CLIENT_INITIALIZE,
            "params": {}
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(response["id"], "bad-version");
    assert_eq!(response["error"]["error"]["code"], "invalidRequest");
    assert_eq!(
        response["error"]["error"]["message"],
        "Invalid request: jsonrpc must be 2.0"
    );
}

#[test]
fn explicit_null_id_returns_invalid_request_error() {
    let (_temp, mut dispatcher) = dispatcher();

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": null,
            "method": CLIENT_INITIALIZE,
            "params": {}
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert!(response["id"].is_null());
    assert_eq!(response["error"]["error"]["code"], "invalidRequest");
    assert_eq!(
        response["error"]["error"]["message"],
        "Invalid request: invalid JSON-RPC id"
    );
}

#[test]
fn object_id_returns_invalid_request_error() {
    let (_temp, mut dispatcher) = dispatcher();

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": { "bad": true },
            "method": CLIENT_INITIALIZE,
            "params": {}
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert!(response["id"].is_null());
    assert_eq!(response["error"]["error"]["code"], "invalidRequest");
    assert_eq!(
        response["error"]["error"]["message"],
        "Invalid request: invalid JSON-RPC id"
    );
}

#[test]
fn notifications_do_not_emit_responses() {
    let (_temp, mut dispatcher) = dispatcher();

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "method": CLIENT_INITIALIZE,
            "params": {}
        })
        .to_string(),
    );

    assert!(responses.is_empty());
}

fn task_record(task_id: &str) -> TaskRecord {
    let workspace_root = "/tmp/openaide-stdio-workspace/a";
    std::fs::create_dir_all(workspace_root).unwrap();
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitle::new(
            "Task",
            crate::storage::records::TaskTitleSource::User,
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace_root.to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Visible,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
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

fn open_store_after_dispatcher_drop(path: &std::path::Path) -> Store {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match Store::open(path.to_path_buf()) {
            Ok(store) => return store,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("store should reopen after dispatcher drop: {error}"),
        }
    }
}

fn completed_task_event(line: &str) -> bool {
    let value = serde_json::from_str::<Value>(line).expect("event json");
    value["method"] == "app/event"
        && value["params"]["payload"]["kind"] == "taskChanged"
        && value["params"]["payload"]["taskId"] == "task-existing"
        && value["params"]["payload"]["changes"]["task"]["status"] == "idle"
}

fn event_payload_kind(line: &str, kind: &str) -> bool {
    let value = response(line);
    value["method"] == "app/event" && value["params"]["payload"]["kind"] == kind
}

fn wait_for_server_request(
    dispatcher: &mut ProtocolEdgeStdioDispatcher,
    notifications: &mpsc::Receiver<TaskUpdate>,
    method: &str,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        let notification = match notifications.recv_timeout(Duration::from_millis(50)) {
            Ok(notification) => notification,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => panic!("task update channel closed: {error}"),
        };
        for line in dispatcher.handle_task_update(notification) {
            let value = response(&line);
            if value["method"] == method {
                return value;
            }
        }
    }
    panic!("server request {method} was not emitted");
}

fn wait_for_protocol_task_status(
    dispatcher: &mut ProtocolEdgeStdioDispatcher,
    notifications: &mpsc::Receiver<TaskUpdate>,
    task_id: &str,
    status: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        let notification = match notifications.recv_timeout(Duration::from_millis(50)) {
            Ok(notification) => notification,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => panic!("task update channel closed: {error}"),
        };
        for line in dispatcher.handle_task_update(notification) {
            let value = response(&line);
            let payload = &value["params"]["payload"];
            if value["method"] == "app/event"
                && payload["kind"] == "taskChanged"
                && payload["taskId"] == task_id
                && payload["changes"]["task"]["status"] == status
            {
                return;
            }
        }
    }
    panic!("task {task_id} did not reach {status}");
}
