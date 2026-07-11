use super::*;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::ProtocolErrorCode;
use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::methods::{
    AGENT_CREATE_CUSTOM, AGENT_DELETE_CUSTOM, AGENT_PROBE, AGENT_REPLACE_CUSTOM, AGENT_SET_ENABLED,
    AGENT_UPDATE_CUSTOM_METADATA, ATTACHMENT_CONFIRM_EMBEDDED,
    ATTACHMENT_CREATE_EMBEDDED_CANDIDATE, ATTACHMENT_CREATE_FILE_REFERENCE,
    ATTACHMENT_CREATE_PASTED_IMAGE, ATTACHMENT_LIST_DIRECTORY, ATTACHMENT_LIST_ROOTS,
    ATTACHMENT_REFRESH_HANDLES, ATTACHMENT_RELEASE_HANDLES, ATTACHMENT_REVEAL, CLIENT_HEARTBEAT,
    CLIENT_INITIALIZE, SETTINGS_GET_AGENT_DETAILS, STATE_SUBSCRIBE, TASK_ADOPT_NATIVE_SESSION,
    TASK_CANCEL, TASK_CREATE, TASK_DISCARD, TASK_LIST, TASK_OPEN, TASK_SEND, TASK_SET_ARCHIVED,
    TASK_SET_CONFIG_OPTION,
};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use openaide_app_server_protocol::state::{StateSubscribeParams, SubscriptionScope};

use crate::projects::project_id_for_workspace;
use crate::protocol::model::{IsolationKind, NormalizedMessage, PermissionDecision, TaskStatus};
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
        } => error,
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
        response["result"]["result"]["snapshot"]["agents"]["defaultAgentId"],
        "codex"
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
fn attachment_handle_is_scoped_to_its_originating_client_at_protocol_boundary() {
    let temp = tempfile::TempDir::new().expect("temp dir");
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
            serde_json::to_value(initialize_params("client-owner")).unwrap(),
        ),
        AppServerTime(1),
    ));
    gateway_result(gateway.handle_inbound(
        other_connection.clone(),
        gateway_request(
            "initialize-other",
            CLIENT_INITIALIZE,
            serde_json::to_value(initialize_params("client-other")).unwrap(),
        ),
        AppServerTime(2),
    ));

    let created = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "create",
            ATTACHMENT_CREATE_PASTED_IMAGE,
            json!({
                "taskId": "task-1",
                "label": "image.png",
                "mimeType": "image/png",
                "data": "aW1hZ2U=",
            }),
        ),
        AppServerTime(3),
    ));
    let handle_id = created["attachment"]["handleId"]
        .as_str()
        .expect("attachment handle")
        .to_string();

    let refresh_error = gateway_error(gateway.handle_inbound(
        other_connection.clone(),
        gateway_request(
            "refresh-other",
            ATTACHMENT_REFRESH_HANDLES,
            json!({ "taskId": "task-1", "handles": [handle_id] }),
        ),
        AppServerTime(4),
    ));
    assert_eq!(
        refresh_error.error.code,
        ProtocolErrorCode::ValidationFailed
    );

    let released = gateway_result(gateway.handle_inbound(
        other_connection.clone(),
        gateway_request(
            "release-other",
            ATTACHMENT_RELEASE_HANDLES,
            json!({ "taskId": "task-1", "handles": [handle_id] }),
        ),
        AppServerTime(5),
    ));
    assert_eq!(released["releasedHandles"], json!([]));

    let send_error = gateway_error(gateway.handle_inbound(
        other_connection,
        gateway_request(
            "send-other",
            TASK_SEND,
            json!({
                "taskId": "task-1",
                "idempotencyKey": "other-send",
                "taskRevision": 1,
                "message": { "text": "inspect", "attachments": [handle_id] },
            }),
        ),
        AppServerTime(6),
    ));
    assert_eq!(send_error.error.code, ProtocolErrorCode::ValidationFailed);

    let refreshed = gateway_result(gateway.handle_inbound(
        owner_connection,
        gateway_request(
            "refresh-owner",
            ATTACHMENT_REFRESH_HANDLES,
            json!({ "taskId": "task-1", "handles": [handle_id] }),
        ),
        AppServerTime(7),
    ));
    assert_eq!(refreshed["attachments"][0]["handleId"], handle_id);
    drop(gateway);
    drop(dispatcher);
    assert!(open_store_after_dispatcher_drop(temp.path())
        .read_messages("task-1")
        .unwrap()
        .is_empty());
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

    let owner_confirmation = gateway_result(gateway.handle_inbound(
        owner_connection.clone(),
        gateway_request(
            "confirm-owner",
            ATTACHMENT_CONFIRM_EMBEDDED,
            json!({ "taskId": "task-1", "candidates": [candidate_id] }),
        ),
        AppServerTime(8),
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
        AppServerTime(9),
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
    let expired = gateway.expire_inactive_clients(AppServerTime(10_002));
    assert_eq!(expired.len(), 1);

    let replacement_connection = ConnectionId::new("conn-replacement");
    gateway_result(gateway.handle_inbound(
        replacement_connection.clone(),
        gateway_request(
            "initialize-replacement",
            CLIENT_INITIALIZE,
            initialize_params("client-reused"),
        ),
        AppServerTime(10_003),
    ));
    let refresh_error = gateway_error(gateway.handle_inbound(
        replacement_connection,
        gateway_request(
            "refresh",
            ATTACHMENT_REFRESH_HANDLES,
            json!({ "taskId": "task-1", "handles": [handle_id] }),
        ),
        AppServerTime(10_004),
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
        response["result"]["result"]["snapshot"]["agents"]["defaultAgentId"],
        "custom.local"
    );
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
        response["result"]["result"]["snapshot"]["tasks"]["tasks"][0]["taskId"],
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
        store.write_task(&task_record("task-existing")).unwrap();
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
            "method": TASK_CREATE,
            "params": {
                "agentId": custom_agent_id,
                "projectId": project_id_for_workspace("/workspace/a"),
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
fn initialize_returns_error_when_storage_backed_snapshot_fails() {
    let (temp, mut dispatcher) = dispatcher();
    std::fs::remove_dir_all(temp.path().join("tasks")).unwrap();

    let responses = dispatcher.handle_line(&init_request("1", "client-1"));

    let response = response(&responses[0]);
    assert_eq!(response["error"]["error"]["code"], "internal");
    assert_eq!(response["error"]["error"]["recoverable"], true);
    assert!(response["error"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Failed to read project collection"));
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
    let task_json = std::fs::read_to_string(temp.path().join("tasks/task-1/task.json")).unwrap();
    let stored_task: serde_json::Value = serde_json::from_str(&task_json).unwrap();
    assert_eq!(stored_task["unread"], false);
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
        "Task"
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
    assert_eq!(update["params"]["payload"]["kind"], "taskSnapshotUpdated");
    assert!(update["params"]["payload"]["task"]
        .get("pendingRequests")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty));
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
            "method": TASK_CREATE,
            "params": {
                "projectId": project_id_for_workspace("/workspace/a"),
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
    assert!(task_id.starts_with("task_"));
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
    assert!(!record.first_prompt_sent);
    assert_eq!(record.active_turn_id, None);
}

#[test]
fn task_create_emits_project_collection_update_after_initialize() {
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
            "method": TASK_CREATE,
            "params": {
                "projectId": project_id_for_workspace("/workspace/a"),
                "agentId": "codex"
            }
        })
        .to_string(),
    );

    let event = app_event_payload(&responses, "projectCollectionUpdated")
        .expect("project collection update");
    assert!(event["projects"]["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .any(|project| project["projectId"] == project_id_for_workspace("/workspace/a").as_str()));
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

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "adopt",
            "method": TASK_ADOPT_NATIVE_SESSION,
            "params": {
                "projectId": project_id_for_workspace("/workspace/a"),
                "agentId": "codex",
                "nativeSessionId": "native-session-1",
                "title": "Imported native session"
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
        "Imported native session"
    );
    assert_eq!(
        response["result"]["result"]["task"]["chat"]["items"][0]["parts"][0]["text"],
        "Mock loaded session."
    );
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    let record = store.read_task(&task_id).unwrap();
    assert_eq!(record.agent_session_id.as_deref(), Some("native-session-1"));
    assert!(record.first_prompt_sent);
    assert!(matches!(record.preparation, TaskPreparationRecord::Ready));
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
                "idempotencyKey": "send-1",
                "taskRevision": 1,
                "message": { "text": "hello from protocol edge" }
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert!(responses
        .iter()
        .skip(1)
        .any(|line| serde_json::from_str::<Value>(line).unwrap()["method"] == "app/event"));
    assert_eq!(
        response["result"]["result"]["task"]["task"]["status"],
        "running"
    );
    assert_eq!(
        response["result"]["result"]["task"]["chat"]["items"][0]["parts"][0]["text"],
        "hello from protocol edge"
    );
    assert!(response["result"]["result"]["turnId"]
        .as_str()
        .unwrap()
        .starts_with("turn_"));
    drop(dispatcher);
    let store = open_store_after_dispatcher_drop(temp.path());
    let record = store.read_task("task-existing").unwrap();
    assert!(record.first_prompt_sent);
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
                scope: SubscriptionScope::TaskNavigation { project_id: None },
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
                "idempotencyKey": "send-1",
                "taskRevision": 1,
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
        if messages.iter().any(completed_task_event) {
            saw_completion_event = true;
            break;
        }
    }

    assert!(saw_completion_event);
}

#[test]
fn task_update_notification_emits_full_snapshot_for_task_subscribers() {
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
        delta: None,
    });

    assert!(messages
        .iter()
        .any(|line| event_payload_kind(line, "taskUpdated")));
    assert!(messages
        .iter()
        .any(|line| event_payload_kind(line, "taskSnapshotUpdated")));
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
                "idempotencyKey": "send-permission",
                "taskRevision": 1,
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
            && value["params"]["payload"]["kind"] == "taskSnapshotUpdated"
    }));

    wait_for_protocol_task_status(&mut dispatcher, &notifications, "task-existing", "idle");
    drop(dispatcher);
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let permission = store
        .read_messages("task-existing")
        .unwrap()
        .into_iter()
        .find_map(|message| match message.chat.message {
            NormalizedMessage::Permission {
                app_server_request_id,
                selected_option,
                decision,
                ..
            } => Some((app_server_request_id, selected_option, decision)),
            _ => None,
        })
        .expect("permission message");
    assert_eq!(permission.0.as_deref(), Some("server-request-1"));
    assert_eq!(permission.1, Some(option_id));
    assert_eq!(permission.2, Some(PermissionDecision::Approved));
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
                "idempotencyKey": "send-permission-deny",
                "taskRevision": 1,
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
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let permission = store
        .read_messages("task-existing")
        .unwrap()
        .into_iter()
        .find_map(|message| match message.chat.message {
            NormalizedMessage::Permission {
                app_server_request_id,
                selected_option,
                decision,
                ..
            } => Some((app_server_request_id, selected_option, decision)),
            _ => None,
        })
        .expect("permission message");
    assert_eq!(permission.0.as_deref(), Some("server-request-1"));
    assert_eq!(permission.1, Some(option_id));
    assert_eq!(permission.2, Some(PermissionDecision::Denied));
}

#[test]
fn attachment_file_browser_creates_handle_used_by_task_send() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("notes.md"), "hello").unwrap();
    let task_id = {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut task = task_record("task-existing");
        task.workspace_root = workspace.to_string_lossy().to_string();
        store.write_task(&task).unwrap();
        "task-existing".to_string()
    };
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let roots = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "roots",
            "method": ATTACHMENT_LIST_ROOTS,
            "params": { "taskId": task_id }
        })
        .to_string(),
    );
    let root_id = response(&roots[0])["result"]["result"]["roots"][0]["rootId"]
        .as_str()
        .unwrap()
        .to_string();

    let listing = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "list",
            "method": ATTACHMENT_LIST_DIRECTORY,
            "params": { "taskId": task_id, "rootId": root_id }
        })
        .to_string(),
    );
    let list_response = response(&listing[0]);
    let entry = list_response["result"]["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["label"] == "notes.md")
        .expect("notes entry");
    let entry_id = entry["entryId"].as_str().unwrap().to_string();
    assert_eq!(entry["selectable"], true);

    let created = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "create-attachment",
            "method": ATTACHMENT_CREATE_FILE_REFERENCE,
            "params": { "taskId": task_id, "entryId": entry_id }
        })
        .to_string(),
    );
    let handle_id = response(&created[0])["result"]["result"]["attachment"]["handleId"]
        .as_str()
        .unwrap()
        .to_string();

    let refreshed = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "refresh-attachment",
            "method": ATTACHMENT_REFRESH_HANDLES,
            "params": { "taskId": task_id, "handles": [handle_id] }
        })
        .to_string(),
    );
    assert_eq!(
        response(&refreshed[0])["result"]["result"]["attachments"][0]["label"],
        "notes.md"
    );

    let sent = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "send",
            "method": TASK_SEND,
            "params": {
                "taskId": task_id,
                "idempotencyKey": "send-with-attachment",
                "taskRevision": 1,
                "message": {
                    "text": "Use this context",
                    "attachments": [handle_id]
                }
            }
        })
        .to_string(),
    );

    let send_response = response(&sent[0]);
    let parts = send_response["result"]["result"]["task"]["chat"]["items"][0]["parts"]
        .as_array()
        .unwrap();
    assert_eq!(parts[1]["kind"], "attachment");
    assert_eq!(parts[1]["attachment"]["label"], "notes.md");
    assert!(parts[1]["attachment"].get("path").is_none());

    let released = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "release-attachment",
            "method": ATTACHMENT_RELEASE_HANDLES,
            "params": { "taskId": task_id, "handles": [handle_id] }
        })
        .to_string(),
    );
    assert!(
        response(&released[0])["result"]["result"]["releasedHandles"]
            .as_array()
            .unwrap()
            .is_empty()
    );
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
fn task_set_config_option_persists_idle_task_option_after_initialize() {
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
                "value": "gpt-5.5",
                "clientMutationId": "mutation-1"
            }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["task"]["agentConfig"]["state"],
        "ready"
    );
    assert_eq!(
        response["result"]["result"]["task"]["agentConfig"]["options"][0]["configId"],
        "model"
    );
    assert_eq!(
        response["result"]["result"]["task"]["agentConfig"]["options"][0]["currentValue"],
        "gpt-5.5"
    );
    drop(dispatcher);
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    assert_eq!(
        store
            .read_task("task-existing")
            .unwrap()
            .config_options
            .get("model"),
        Some(&"gpt-5.5".to_string())
    );
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
fn task_discard_hides_empty_pre_send_task_after_initialize() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store.write_task(&task_record("task-draft")).unwrap();
        let mut existing = task_record("task-existing");
        existing.first_prompt_sent = true;
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
            "method": TASK_DISCARD,
            "params": { "taskId": "task-draft" }
        })
        .to_string(),
    );

    let response = response(&responses[0]);
    assert_eq!(
        response["result"]["result"]["discardedTaskId"],
        "task-draft"
    );
    let tasks = response["result"]["result"]["tasks"]["tasks"]
        .as_array()
        .expect("tasks");
    assert_eq!(tasks.len(), 1);
    assert!(tasks.iter().any(|task| task["taskId"] == "task-existing"));
    assert!(responses
        .iter()
        .skip(1)
        .any(|line| serde_json::from_str::<Value>(line).unwrap()["method"] == "app/event"));
    drop(dispatcher);
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    assert!(store.read_task("task-draft").unwrap().tombstoned);
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
    assert_eq!(
        archive_response["result"]["result"]["tasks"]["tasks"]
            .as_array()
            .expect("active tasks after archive")
            .len(),
        0
    );

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
fn task_discard_emits_project_collection_update_after_last_project_task() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        let mut draft = task_record("task-draft");
        draft.workspace_root = "/workspace/draft-only".to_string();
        store.write_task(&draft).unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
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
            "method": TASK_DISCARD,
            "params": { "taskId": "task-draft" }
        })
        .to_string(),
    );

    let event = app_event_payload(&responses, "projectCollectionUpdated")
        .expect("project collection update");
    assert!(event["projects"]["projects"]
        .as_array()
        .expect("projects")
        .is_empty());
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
    TaskRecord {
        task_id: task_id.to_string(),
        title: "Task".to_string(),
        agent_title: None,
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace/a".to_string(),
        first_prompt_sent: false,
        agent_session_id: None,
        active_turn_id: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options: Default::default(),
        config_options_catalog: None,
        agent_commands_catalog: None,
        model_id: None,
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

fn completed_task_event(line: &String) -> bool {
    let value = serde_json::from_str::<Value>(line).expect("event json");
    value["method"] == "app/event"
        && value["params"]["payload"]["task"]["taskId"] == "task-existing"
        && value["params"]["payload"]["task"]["status"] != "running"
}

fn event_payload_kind(line: &str, kind: &str) -> bool {
    let value = response(line);
    value["method"] == "app/event" && value["params"]["payload"]["kind"] == kind
}

fn app_event_payload(lines: &[String], kind: &str) -> Option<Value> {
    lines.iter().skip(1).find_map(|line| {
        let value = serde_json::from_str::<Value>(line).ok()?;
        (value["method"] == "app/event" && value["params"]["payload"]["kind"] == kind)
            .then(|| value["params"]["payload"].clone())
    })
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
        while let Ok(notification) = notifications.try_recv() {
            let _ = dispatcher.handle_task_update(notification);
        }
        let responses = dispatcher.handle_line(
            &json!({
                "jsonrpc": "2.0",
                "id": "poll-task",
                "method": TASK_OPEN,
                "params": { "taskId": task_id }
            })
            .to_string(),
        );
        if response(&responses[0])["result"]["result"]["task"]["task"]["status"] == status {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("task {task_id} did not reach {status}");
}
