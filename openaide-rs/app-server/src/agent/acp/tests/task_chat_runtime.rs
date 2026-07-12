use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use openaide_app_server_protocol::ids::{AgentId, TaskId};
use openaide_app_server_protocol::snapshot::{ChatItem, ChatItemStatus, ChatRole, MessagePart};
use openaide_app_server_protocol::task::{
    ComposerMessage, TaskAdoptNativeSessionParams, TaskCreateParams, TaskSendParams,
};

use crate::agent::acp::{AcpAgentConfig, AcpAgentRuntime};
use crate::agent::registry::AgentRegistry;
use crate::projects::{project_id_for_workspace, ConfiguredProjectRoots, StorageProjectResolver};
use crate::protocol::model::{NormalizedMessage, TaskStatus};
use crate::server_requests::ServerRequestRuntime;
use crate::storage::records::TaskPreparationRecord;
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::product_api::{
    TaskAdoptNativeSessionWorkflow, TaskCreateWorkflow, TaskOpenWorkflow, TaskProductApi,
};

#[test]
fn live_acp_message_ids_create_separate_chat_messages() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "message_ids") else {
        return;
    };
    let created = api
        .create(TaskCreateParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            workspace_root: None,
            config_options: Default::default(),
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
    let ready = api
        .open(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open ready task");

    api.send(send_params(
        &task_id,
        ready.revision,
        "send-message-ids",
        "respond twice",
    ))
    .expect("send prompt");

    wait_until(|| {
        store
            .read_task(task_id.as_str())
            .map(|task| task.status == TaskStatus::Inactive)
            .unwrap_or(false)
    });
    let completed = api
        .open(openaide_app_server_protocol::task::TaskOpenParams {
            task_id: task_id.clone(),
        })
        .expect("open completed task");
    assert_eq!(
        agent_text_statuses(&completed.chat.items),
        [
            ("Commentary message".to_string(), ChatItemStatus::Complete),
            ("Final message".to_string(), ChatItemStatus::Complete),
        ]
    );
    api.shutdown().expect("shutdown task runtime");
}

#[test]
fn replayed_acp_chunks_use_live_logical_message_grouping() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let Some((api, store, workspace_root)) = task_chat_fixture(&temp, "replay") else {
        return;
    };

    let adopted = api
        .adopt_native_session(TaskAdoptNativeSessionParams {
            project_id: project_id_for_workspace(&workspace_root),
            agent_id: AgentId::from("codex"),
            native_session_id: "task-chat-session".to_string(),
            title: Some("Replay grouping".to_string()),
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
    let identities = store
        .read_messages(task_id.as_str())
        .expect("read replayed messages")
        .into_iter()
        .map(|stored| stored.chat.identity)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(identities.len(), 4);
    api.shutdown().expect("shutdown task runtime");
}

fn task_chat_fixture(
    temp: &tempfile::TempDir,
    mode: &str,
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
        ServerRequestRuntime::new(),
    )
    .expect("task product api");
    Some((api, store, workspace_root))
}

fn send_params(task_id: &TaskId, revision: u64, key: &str, text: &str) -> TaskSendParams {
    TaskSendParams {
        task_id: task_id.clone(),
        idempotency_key: key.into(),
        task_revision: revision,
        message: ComposerMessage {
            text: Some(text.to_string()),
            attachments: Vec::new(),
        },
    }
}

fn agent_text_statuses(items: &[ChatItem]) -> Vec<(String, ChatItemStatus)> {
    items
        .iter()
        .filter(|item| item.role == ChatRole::Agent)
        .filter_map(|item| match item.parts.first() {
            Some(MessagePart::Text { text }) => Some((text.clone(), item.status)),
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
            NormalizedMessage::AgentText { text, .. } => Some(("agent", text)),
            NormalizedMessage::Thought { text, .. } => Some(("thought", text)),
            _ => None,
        })
        .collect()
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for predicate");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn task_chat_agent_script() -> &'static str {
    r#"import json
import os
import sys

mode = os.environ.get("OPENAIDE_TASK_CHAT_MODE", "message_ids")
session_id = "task-chat-session"

def write(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()

def respond(message, result):
    write({"jsonrpc": "2.0", "id": message.get("id"), "result": result})

def update_chunk(kind, text, message_id):
    write({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": kind,
                "messageId": message_id,
                "content": {"type": "text", "text": text},
            },
        },
    })

for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        respond(message, {
            "protocolVersion": 1,
            "agentCapabilities": {
                "loadSession": True,
                "sessionCapabilities": {"close": {}},
            },
            "authMethods": [],
        })
    elif method == "session/new":
        respond(message, {"sessionId": session_id})
    elif method == "session/load":
        if mode == "replay":
            update_chunk("user_message_chunk", "Prior ", "33333333-3333-4333-8333-333333333333")
            update_chunk("user_message_chunk", "question", "33333333-3333-4333-8333-333333333333")
            update_chunk("agent_message_chunk", "First ", "44444444-4444-4444-8444-444444444444")
            update_chunk("agent_message_chunk", "answer", "44444444-4444-4444-8444-444444444444")
            update_chunk("agent_message_chunk", "Final ", "55555555-5555-4555-8555-555555555555")
            update_chunk("agent_message_chunk", "answer", "55555555-5555-4555-8555-555555555555")
            update_chunk("agent_thought_chunk", "Work ", "66666666-6666-4666-8666-666666666666")
            update_chunk("agent_thought_chunk", "it out", "66666666-6666-4666-8666-666666666666")
        respond(message, {"configOptions": []})
    elif method == "session/prompt":
        update_chunk("agent_message_chunk", "Commentary ", "11111111-1111-4111-8111-111111111111")
        update_chunk("agent_message_chunk", "message", "11111111-1111-4111-8111-111111111111")
        update_chunk("agent_message_chunk", "Final ", "22222222-2222-4222-8222-222222222222")
        update_chunk("agent_message_chunk", "message", "22222222-2222-4222-8222-222222222222")
        respond(message, {"stopReason": "end_turn"})
    elif method == "session/close":
        respond(message, {})
        break
"#
}
