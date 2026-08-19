use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use openaide_app_server_protocol::agent::AgentListSessionsParams;
use openaide_app_server_protocol::ids::{AgentId, TaskId};
use openaide_app_server_protocol::snapshot::{ChatItem, ChatItemStatus, ChatRole, MessagePart};
use openaide_app_server_protocol::task::{
    ComposerMessage, TaskAcquireParams, TaskAdoptNativeSessionParams, TaskSendParams,
};

use crate::agent::acp::{AcpAgentConfig, AcpAgentRuntime};
use crate::agent::registry::AgentRegistry;
use crate::attachment_runtime::AttachmentOwner;
use crate::client_lifecycle::{AppServerTime, ConnectionId, Delivery, RequestCapability};
use crate::projects::{project_id_for_workspace, ConfiguredProjectRoots, StorageProjectResolver};
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, NormalizedMessage, TaskStatus};
use crate::server_requests::{ResponderScope, ServerRequestAnswer, ServerRequestRuntime};
use crate::storage::records::TaskPreparationRecord;
use crate::storage::{Store, StoreOpenError};
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::product_api::AgentListSessionsWorkflow;
use crate::tasks::product_api::{TaskAdoptNativeSessionWorkflow, TaskProductApi};

#[test]
fn live_acp_message_ids_create_separate_chat_messages() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "message_ids") else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });
    api.send(send_params(&task_id, "respond twice"))
        .expect("send prompt");

    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    let completed = api
        .open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open completed task");
    assert_eq!(
        agent_text_items(&completed.chat.items),
        [
            (
                "acp:task-chat-session:message:11111111-1111-4111-8111-111111111111".to_string(),
                "Commentary message".to_string(),
                ChatItemStatus::Complete,
            ),
            (
                "acp:task-chat-session:message:22222222-2222-4222-8222-222222222222".to_string(),
                "Final message".to_string(),
                ChatItemStatus::Complete,
            ),
        ]
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn permission_does_not_split_already_received_anonymous_agent_text() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let server_requests = ServerRequestRuntime::new();
    let Some((api, store, workspace_root)) = task_chat_fixture_with_requests(
        &temp,
        "anonymous_permission_burst",
        server_requests.clone(),
    ) else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    register_permission_responder(&server_requests, task_id.as_str());
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    let answerer = std::thread::spawn({
        let server_requests = server_requests.clone();
        let task_id = task_id.clone();
        move || auto_allow_permission(&server_requests, task_id.as_str())
    });
    api.send(send_params(&task_id, "edit the config"))
        .expect("send prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    answerer.join().expect("permission answerer joins");

    assert_eq!(
        visible_chat_rows(&store, &task_id),
        [
            ("user", "edit the config".to_string()),
            (
                "agent",
                "Setting `approvalMode` to `unrestricted`.".to_string()
            ),
            ("activity", "Edit File".to_string()),
            ("agent", "After the edit.".to_string()),
        ]
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn one_acp_message_id_can_cross_visible_and_thought_channels() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) =
        task_chat_fixture(&temp, "shared_message_id_across_channels")
    else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    for prompt in ["first prompt", "second prompt"] {
        api.send(send_params(&task_id, prompt))
            .expect("send prompt");
        wait_until(|| {
            store
                .read_task(task_id.as_str())
                .map(|task| task.status == TaskStatus::Inactive)
                .unwrap_or(false)
        });
    }

    assert_eq!(
        logical_text_messages(&store, &task_id),
        [
            ("user", "first prompt".to_string()),
            (
                "agent",
                "Visible before thoughtVisible after thought".to_string()
            ),
            ("thought", "Private thought".to_string()),
            ("user", "second prompt".to_string()),
            (
                "agent",
                "Visible before thoughtVisible after thought".to_string()
            ),
            ("thought", "Private thought".to_string()),
        ]
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn acp_usage_is_published_in_the_authoritative_task_snapshot() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "usage") else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    api.send(send_params(&task_id, "report usage"))
        .expect("send prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let snapshot = api
        .open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open completed task");
    let snapshot = serde_json::to_value(snapshot).expect("serialize task snapshot");
    assert_eq!(
        snapshot.get("contextUsage"),
        Some(&serde_json::json!({
            "usedTokens": 31_000,
            "capacityTokens": 258_400,
            "cost": {
                "amount": "0.42",
                "currency": "USD",
            },
            "lastTurn": {
                "totalTokens": 168_500,
                "inputTokens": 1_700,
                "outputTokens": 118,
                "reasoningTokens": 14,
                "cachedReadTokens": 166_700,
                "cachedWriteTokens": 86,
            },
        }))
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn incomplete_acp_plan_is_published_in_the_authoritative_task_snapshot() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "incomplete_plan") else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    api.send(send_params(&task_id, "report a plan"))
        .expect("send prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let snapshot = api
        .open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open completed task");
    let snapshot = serde_json::to_value(snapshot).expect("serialize task snapshot");
    assert_eq!(
        snapshot.get("currentPlan"),
        Some(&serde_json::json!({
            "entries": [
                {
                    "content": "Inspect the projection",
                    "priority": "high",
                    "status": "completed",
                },
                {
                    "content": "Persist the replacement snapshot",
                    "priority": "medium",
                    "status": "inProgress",
                },
                {
                    "content": "Render the plan",
                    "priority": "low",
                    "status": "pending",
                },
            ],
        }))
    );
    api.shutdown().expect("shutdown task runtime");
    drop(api);
    drop(store);

    let reopened = reopen_store_after_fixture_shutdown(temp.path().join("store"));
    let restored = crate::tasks::snapshot::build_snapshot(&reopened, task_id.as_str(), 100)
        .expect("restore durable task snapshot");
    assert_eq!(
        restored.current_plan.map(|plan| plan.entries),
        Some(vec![
            crate::protocol::model::AgentPlanEntry {
                content: "Inspect the projection".to_string(),
                priority: crate::protocol::model::AgentPlanPriority::High,
                status: crate::protocol::model::AgentPlanStatus::Completed,
            },
            crate::protocol::model::AgentPlanEntry {
                content: "Persist the replacement snapshot".to_string(),
                priority: crate::protocol::model::AgentPlanPriority::Medium,
                status: crate::protocol::model::AgentPlanStatus::InProgress,
            },
            crate::protocol::model::AgentPlanEntry {
                content: "Render the plan".to_string(),
                priority: crate::protocol::model::AgentPlanPriority::Low,
                status: crate::protocol::model::AgentPlanStatus::Pending,
            },
        ])
    );
}

#[test]
fn completed_acp_plan_moves_once_to_chat_and_later_completed_snapshots_replace_it() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "completed_plan") else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    api.send(send_params(&task_id, "complete a plan"))
        .expect("send prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let snapshot = api
        .open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open completed task");
    let snapshot = serde_json::to_value(snapshot).expect("serialize task snapshot");
    assert_eq!(snapshot.get("currentPlan"), None);
    let completed_parts = snapshot["chat"]["items"]
        .as_array()
        .expect("chat items")
        .iter()
        .flat_map(|item| item["parts"].as_array().expect("chat item parts"))
        .filter(|part| part.get("kind") == Some(&serde_json::json!("completedPlan")))
        .collect::<Vec<_>>();
    assert_eq!(
        completed_parts,
        vec![&serde_json::json!({
            "kind": "completedPlan",
            "entries": [
                {
                    "content": "Inspect the projection",
                    "priority": "high",
                    "status": "completed",
                },
                {
                    "content": "Persist the final replacement",
                    "priority": "low",
                    "status": "completed",
                },
            ],
        })]
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn empty_acp_plan_clears_the_current_plan_without_adding_chat() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "cleared_plan") else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    api.send(send_params(&task_id, "clear the plan"))
        .expect("send prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });

    let snapshot = api
        .open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open completed task");
    let snapshot = serde_json::to_value(snapshot).expect("serialize task snapshot");
    assert_eq!(snapshot.get("currentPlan"), None);
    assert!(snapshot["chat"]["items"]
        .as_array()
        .expect("chat items")
        .iter()
        .flat_map(|item| item["parts"].as_array().expect("chat item parts"))
        .all(|part| part.get("kind") != Some(&serde_json::json!("completedPlan"))));
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn replayed_acp_chunks_use_live_logical_message_grouping() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "replay") else {
        return;
    };

    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace(&workspace_root),
        cursor: None,
    })
    .expect("list replayable ACP session");
    let adopted = api
        .adopt_native_session(TaskAdoptNativeSessionParams {
            agent_id: AgentId::from("codex"),
            native_session_id: "task-chat-session".to_string(),
        })
        .expect("adopt replayed ACP session");
    let task_id = adopted.task.task_id;

    assert_eq!(
        logical_text_messages(&store, &task_id),
        [
            ("user", "Prior question".to_string()),
            ("agent", "First answer".to_string()),
            ("agent", "Final answer".to_string()),
            ("thought", "Work it out".to_string()),
        ]
    );
    assert!(adopted.chat.items.iter().any(|item| {
        matches!(
            item.parts.as_slice(),
            [MessagePart::Image { media_type, .. }] if media_type == "image/png"
        )
    }));
    let identities = store
        .read_messages(task_id.as_str())
        .expect("read replayed messages")
        .into_iter()
        .map(|stored| stored.chat.identity)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(identities.len(), 5);
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn adopting_a_native_session_open_elsewhere_returns_a_recoverable_conflict() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, _store, workspace_root)) = task_chat_fixture(&temp, "active_writer") else {
        return;
    };

    api.list_agent_sessions(AgentListSessionsParams {
        agent_id: AgentId::from("codex"),
        project_id: project_id_for_workspace(&workspace_root),
        cursor: None,
    })
    .expect("list externally owned ACP session");

    let error = api
        .adopt_native_session(TaskAdoptNativeSessionParams {
            agent_id: AgentId::from("codex"),
            native_session_id: "task-chat-session".to_string(),
        })
        .expect_err("an active writer cannot be adopted into this App Server");

    assert_eq!(
        error.code,
        openaide_app_server_protocol::errors::ProtocolErrorCode::Conflict
    );
    assert!(error.recoverable);
    assert_eq!(
        error.message,
        "Native Session is currently in use elsewhere"
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn non_text_acp_output_is_visible_as_typed_chat_parts() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "content_blocks") else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });
    api.send(send_params(&task_id, "show rich output"))
        .expect("send prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    wait_until(|| {
        api.open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .map(|snapshot| {
            snapshot
                .chat
                .items
                .iter()
                .filter(|item| item.role == ChatRole::Agent)
                .count()
                == 6
        })
        .unwrap_or(false)
    });
    let completed = api
        .open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open completed task");
    let agent_parts = completed
        .chat
        .items
        .iter()
        .filter(|item| item.role == ChatRole::Agent)
        .flat_map(|item| item.parts.iter())
        .map(|part| serde_json::to_value(part).expect("serialize Chat part"))
        .collect::<Vec<_>>();

    assert_eq!(
        agent_parts,
        vec![
            serde_json::json!({
                "kind": "image",
                "mediaType": "image/png",
                "dataUrl": "data:image/png;base64,aW1hZ2U=",
                "uri": "memory://diagram.png"
            }),
            serde_json::json!({
                "kind": "resource",
                "uri": "memory://notes.txt",
                "mediaType": "text/plain",
                "text": "Embedded notes"
            }),
            serde_json::json!({
                "kind": "resource",
                "uri": "https://example.test/report.pdf",
                "name": "report.pdf",
                "title": "Report",
                "description": "Generated report",
                "mediaType": "application/pdf",
                "sizeBytes": 42
            }),
            serde_json::json!({
                "kind": "unsupported",
                "contentType": "audio",
                "mediaType": "audio/wav"
            }),
            serde_json::json!({
                "kind": "unsupported",
                "contentType": "embedded_binary_resource",
                "mediaType": "application/octet-stream",
                "uri": "memory://archive.bin"
            }),
            serde_json::json!({
                "kind": "unsupported",
                "contentType": "image",
                "mediaType": "text/html",
                "uri": "memory://not-an-image"
            }),
        ]
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn steering_end_turn_makes_the_task_idle_when_primary_never_returns() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "steering_end_turn") else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    api.send(send_params(&task_id, "start primary work"))
        .expect("send primary prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Active && task.active_turn_id.is_some())
            .unwrap_or(false)
    });
    api.send(send_params(&task_id, "replace it with this"))
        .expect("send steering prompt");

    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive && task.active_turn_id.is_none())
            .unwrap_or(false)
    });
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn steering_keeps_task_active_when_primary_is_cancelled() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "steering_primary_cancelled")
    else {
        return;
    };
    let created = api
        .create_for_test(TaskAcquireParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
        })
        .expect("create task");
    let task_id = created.task.task_id;
    wait_until(|| {
        matches!(
            store
                .read_task(task_id.as_str())
                .map(|task| task.preparation),
            Ok(TaskPreparationRecord::Ready)
        )
    });

    api.send(send_params(&task_id, "start primary work"))
        .expect("send primary prompt");
    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Active && task.active_turn_id.is_some())
            .unwrap_or(false)
    });
    api.send(send_params(&task_id, "redirect the work"))
        .expect("send steering prompt");

    wait_until(|| {
        visible_chat_rows(&store, &task_id)
            .iter()
            .any(|(_, text)| text == "Primary prompt superseded")
    });
    assert_eq!(
        store
            .read_task(task_id.as_str())
            .expect("read active task")
            .status,
        TaskStatus::Active,
        "primary cancellation must not settle an accepted steering turn"
    );

    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive && task.active_turn_id.is_none())
            .unwrap_or(false)
    });
    api.shutdown().expect("shutdown task runtime");
}

fn task_chat_fixture(
    temp: &tempfile::TempDir,
    mode: &str,
) -> Option<(TaskProductApi, Store, String)> {
    task_chat_fixture_with_requests(temp, mode, ServerRequestRuntime::new())
}

fn task_chat_fixture_with_requests(
    temp: &tempfile::TempDir,
    mode: &str,
    server_requests: ServerRequestRuntime,
) -> Option<(TaskProductApi, Store, String)> {
    if Command::new("python3").arg("--version").output().is_err() {
        return None;
    }
    let script_path = temp.path().join("task_chat_agent.py");
    fs::write(&script_path, task_chat_agent_script()).expect("fixture agent script");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace dir");
    let workspace_root = workspace.to_string_lossy().to_string();
    let config = AcpAgentConfig {
        agent_id: "codex".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: vec![("OPENAIDE_TASK_CHAT_MODE".to_string(), mode.to_string())],
        secret_env: Vec::new(),
    };
    let store = Store::open(temp.path().join("store")).expect("store");
    let projects = ConfiguredProjectRoots::from_workspace_roots([workspace_root.clone()]);
    let api = TaskProductApi::new_with_server_requests(
        store.clone(),
        Arc::new(StorageProjectResolver::new_with_configured_roots(
            store.clone(),
            projects,
        )),
        AgentRegistry::codex(config.clone()),
        Arc::new(AcpAgentRuntime::new(config)),
        TaskUpdateNotifier::disabled(),
        server_requests,
    )
    .expect("task product api");
    Some((api, store, workspace_root))
}

fn send_params(task_id: &TaskId, text: &str) -> TaskSendParams {
    TaskSendParams {
        task_id: task_id.clone(),
        queue_selection: None,
        message: ComposerMessage {
            text: Some(text.to_string()),
            images: Vec::new(),
            attachments: Vec::new(),
        },
    }
}

fn agent_text_items(items: &[ChatItem]) -> Vec<(String, String, ChatItemStatus)> {
    items
        .iter()
        .filter(|item| item.role == ChatRole::Agent)
        .filter_map(|item| match item.parts.first() {
            Some(MessagePart::Text { text }) => Some((
                item.message_id.as_str().to_string(),
                text.clone(),
                item.status,
            )),
            _ => None,
        })
        .collect()
}

fn logical_text_messages(store: &Store, task_id: &TaskId) -> Vec<(&'static str, String)> {
    store
        .read_messages(task_id.as_str())
        .expect("read task messages")
        .into_iter()
        .filter_map(|stored| match stored.chat.message {
            NormalizedMessage::User { text, .. } => Some(("user", text)),
            NormalizedMessage::AgentMessage { role, parts, .. } => {
                parts.into_iter().find_map(|part| match part {
                    AgentMessagePart::Text { text } => Some((
                        match role {
                            AgentMessageRole::Agent => "agent",
                            AgentMessageRole::Thought => "thought",
                        },
                        text,
                    )),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

fn visible_chat_rows(store: &Store, task_id: &TaskId) -> Vec<(&'static str, String)> {
    store
        .read_messages(task_id.as_str())
        .expect("read task messages")
        .into_iter()
        .filter_map(|stored| match stored.chat.message {
            NormalizedMessage::User { text, .. } => Some(("user", text)),
            NormalizedMessage::AgentMessage { role, parts, .. } => {
                parts.into_iter().find_map(|part| match part {
                    AgentMessagePart::Text { text } => Some((
                        match role {
                            AgentMessageRole::Agent => "agent",
                            AgentMessageRole::Thought => "thought",
                        },
                        text,
                    )),
                    _ => None,
                })
            }
            NormalizedMessage::Activity { title, .. } if title != "Working" => {
                Some(("activity", title))
            }
            _ => None,
        })
        .collect()
}

fn register_permission_responder(server_requests: &ServerRequestRuntime, task_id: &str) {
    server_requests.observe_subscription_added(
        Delivery::new(
            AttachmentOwner::test_client_instance_id(),
            ConnectionId::new("conn-1"),
        )
        .with_request_capabilities(vec![RequestCapability::Permission]),
        TaskId::from(task_id),
        AppServerTime(0),
    );
}

fn auto_allow_permission(server_requests: &ServerRequestRuntime, task_id: &str) {
    let task_id = TaskId::from(task_id);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let pending = server_requests.pending_for_task(&task_id);
        if let Some(request) = pending.first() {
            assert!(matches!(
                server_requests.handle_response_from_scopes(
                    AttachmentOwner::test_client_instance_id(),
                    request.request_id.clone(),
                    ServerRequestAnswer::Result(serde_json::json!({ "optionId": "allow-once" })),
                    &[ResponderScope::Task(task_id.clone())],
                    AppServerTime(1),
                ),
                crate::server_requests::ResponseOutcome::Accepted { .. }
            ));
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for permission request"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for predicate");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn reopen_store_after_fixture_shutdown(root: std::path::PathBuf) -> Store {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Store::open(root.clone()) {
            Ok(store) => return store,
            Err(StoreOpenError::LockedByLiveServer) if Instant::now() < deadline => {
                // Allow detached ACP fixture teardown to release its last Store clone.
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("reopen store after fixture shutdown: {error}"),
        }
    }
}

fn task_chat_agent_script() -> &'static str {
    r#"import json
import os
import sys
import time

mode = os.environ.get("OPENAIDE_TASK_CHAT_MODE", "message_ids")
session_id = "task-chat-session"
prompt_count = 0
pending_primary_id = None
pending_prompt_id = None
permission_rpc_id = "permission-1"

def write(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

def respond(message, result):
    write({"jsonrpc": "2.0", "id": message.get("id"), "result": result})

def update_chunk(kind, text, message_id):
    payload = {
        "sessionUpdate": kind,
        "content": {"type": "text", "text": text},
    }
    if message_id is not None:
        payload["messageId"] = message_id
    write({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": payload,
        },
    })

def update_content(content, message_id):
    write({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "messageId": message_id,
                "content": content,
            },
        },
    })

def update_usage():
    write({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": "usage_update",
                "used": 31000,
                "size": 258400,
                "cost": {"amount": 0.42, "currency": "USD"},
            },
        },
    })

def update_plan(entries):
    write({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": "plan",
                "entries": entries,
            },
        },
    })

for line in sys.stdin:
    message = json.loads(line)
    if message.get("id") == permission_rpc_id:
        if pending_prompt_id is not None:
            update_chunk("agent_message_chunk", "After the edit.", None)
            respond({"id": pending_prompt_id}, {"stopReason": "end_turn"})
            pending_prompt_id = None
        continue
    method = message.get("method")
    if method == "initialize":
        respond(message, {
            "protocolVersion": 1,
            "agentCapabilities": {
                "loadSession": True,
                "sessionCapabilities": {"close": {}, "list": {}},
            },
            "authMethods": [],
        })
    elif method == "session/new":
        respond(message, {"sessionId": session_id})
    elif method == "session/list":
        cwd = message.get("params", {}).get("cwd") or os.getcwd()
        respond(message, {"sessions": [{"sessionId": session_id, "cwd": cwd, "title": "Task chat session"}]})
    elif method == "session/load":
        if mode == "active_writer":
            write({
                "jsonrpc": "2.0",
                "id": message.get("id"),
                "error": {
                    "code": -32603,
                    "message": "Internal error: {\"details\":\"thread active-writer-fixture already has an active writer\"}",
                },
            })
            continue
        if mode == "replay":
            update_chunk("user_message_chunk", "Prior ", "33333333-3333-4333-8333-333333333333")
            update_chunk("user_message_chunk", "question", "33333333-3333-4333-8333-333333333333")
            update_chunk("agent_message_chunk", "First ", "44444444-4444-4444-8444-444444444444")
            update_chunk("agent_message_chunk", "answer", "44444444-4444-4444-8444-444444444444")
            update_chunk("agent_message_chunk", "Final ", "55555555-5555-4555-8555-555555555555")
            update_chunk("agent_message_chunk", "answer", "55555555-5555-4555-8555-555555555555")
            update_chunk("agent_thought_chunk", "Work ", "66666666-6666-4666-8666-666666666666")
            update_chunk("agent_thought_chunk", "it out", "66666666-6666-4666-8666-666666666666")
            update_content({"type": "image", "mimeType": "image/png", "data": "aW1hZ2U="}, "replayed-image")
        respond(message, {"configOptions": []})
    elif method == "session/prompt":
        prompt_count += 1
        if mode == "steering_end_turn" and prompt_count == 1:
            pending_primary_id = message.get("id")
            continue
        if mode == "steering_primary_cancelled" and prompt_count == 1:
            pending_primary_id = message.get("id")
            continue
        if mode == "steering_primary_cancelled" and prompt_count == 2:
            respond({"id": pending_primary_id}, {"stopReason": "cancelled"})
            pending_primary_id = None
            time.sleep(0.2)
            update_chunk("agent_message_chunk", "Primary prompt superseded", "superseded")
            time.sleep(0.5)
            respond(message, {"stopReason": "end_turn"})
            continue
        if mode == "content_blocks":
            update_content({"type": "image", "mimeType": "image/png", "data": "aW1hZ2U=", "uri": "memory://diagram.png"}, "content-image")
            update_content({"type": "resource", "resource": {"uri": "memory://notes.txt", "mimeType": "text/plain", "text": "Embedded notes"}}, "content-text-resource")
            update_content({"type": "resource_link", "uri": "https://example.test/report.pdf", "name": "report.pdf", "title": "Report", "description": "Generated report", "mimeType": "application/pdf", "size": 42}, "content-resource-link")
            update_content({"type": "audio", "mimeType": "audio/wav", "data": "YXVkaW8="}, "content-audio")
            update_content({"type": "resource", "resource": {"uri": "memory://archive.bin", "mimeType": "application/octet-stream", "blob": "YmluYXJ5"}}, "content-binary-resource")
            update_content({"type": "image", "mimeType": "text/html", "data": "PGh0bWw+", "uri": "memory://not-an-image"}, "content-invalid-image")
        elif mode == "incomplete_plan":
            update_plan([
                {"content": "Old step that must disappear", "priority": "medium", "status": "pending"},
            ])
            update_plan([
                {"content": "Inspect the projection", "priority": "high", "status": "completed"},
                {"content": "Persist the replacement snapshot", "priority": "medium", "status": "in_progress"},
                {"content": "Render the plan", "priority": "low", "status": "pending"},
            ])
        elif mode == "completed_plan":
            update_plan([
                {"content": "Inspect the projection", "priority": "high", "status": "in_progress"},
                {"content": "Persist the replacement snapshot", "priority": "medium", "status": "pending"},
                {"content": "Remove this step", "priority": "low", "status": "pending"},
            ])
            update_plan([
                {"content": "Inspect the projection", "priority": "high", "status": "completed"},
                {"content": "Persist the replacement snapshot", "priority": "medium", "status": "completed"},
            ])
            update_plan([
                {"content": "Inspect the projection", "priority": "high", "status": "completed"},
                {"content": "Persist the final replacement", "priority": "low", "status": "completed"},
            ])
        elif mode == "cleared_plan":
            update_plan([
                {"content": "Temporary step", "priority": "medium", "status": "in_progress"},
            ])
            update_plan([])
        elif mode == "shared_message_id_across_channels":
            message_id = f"shared-message-{prompt_count}"
            update_chunk("agent_message_chunk", "Visible before thought", message_id)
            update_chunk("agent_thought_chunk", "Private thought", message_id)
            update_chunk("agent_message_chunk", "Visible after thought", message_id)
        elif mode == "anonymous_permission_burst":
            for token in [
                "Setting", " `", "approval", "Mode", "`",
                " to ", "`", "unrestricted", "`.",
            ]:
                update_chunk("agent_message_chunk", token, None)
            write({
                "jsonrpc": "2.0",
                "id": permission_rpc_id,
                "method": "session/request_permission",
                "params": {
                    "sessionId": session_id,
                    "toolCall": {
                        "toolCallId": "edit-1",
                        "title": "Edit File",
                        "kind": "edit",
                        "status": "pending",
                    },
                    "options": [{
                        "optionId": "allow-once",
                        "name": "Allow once",
                        "kind": "allow_once",
                    }],
                },
            })
            pending_prompt_id = message.get("id")
            continue
        else:
            update_chunk("agent_message_chunk", "Commentary ", "11111111-1111-4111-8111-111111111111")
            update_chunk("agent_message_chunk", "message", "11111111-1111-4111-8111-111111111111")
            update_chunk("agent_message_chunk", "Final ", "22222222-2222-4222-8222-222222222222")
            update_chunk("agent_message_chunk", "message", "22222222-2222-4222-8222-222222222222")
        if mode == "usage":
            update_usage()
            respond(message, {
                "stopReason": "end_turn",
                "usage": {
                    "totalTokens": 168500,
                    "inputTokens": 1700,
                    "outputTokens": 118,
                    "thoughtTokens": 14,
                    "cachedReadTokens": 166700,
                    "cachedWriteTokens": 86,
                },
            })
        else:
            respond(message, {"stopReason": "end_turn"})
    elif method == "session/close":
        if pending_primary_id is not None:
            respond({"id": pending_primary_id}, {"stopReason": "end_turn"})
        respond(message, {})
        break
"#
}
