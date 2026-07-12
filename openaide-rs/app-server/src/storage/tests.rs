use super::*;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, ActivityToolContent, ActivityToolDetails, ChatMessage,
    IsolationKind, NormalizedMessage, TaskStatus,
};
use crate::storage::records::{StoredMessage, TaskPreparationRecord, TaskRecord};
use crate::storage_runtime::RecoveryClassification;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[test]
fn second_store_open_is_blocked_while_first_store_lives() {
    let dir = tempfile::tempdir().unwrap();
    let _first = Store::open(dir.path().to_path_buf()).unwrap();

    assert!(matches!(
        Store::open(dir.path().to_path_buf()),
        Err(StoreOpenError::LockedByLiveServer)
    ));
}

#[test]
fn task_title_persists_as_one_owned_value() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let mut task = task_record("task-title", TaskStatus::Inactive, "1");
    task.title = crate::storage::records::TaskTitle::new(
        "  Agent title  ",
        crate::storage::records::TaskTitleSource::Agent,
    );

    store.write_task(&task).unwrap();

    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(store.task_dir("task-title").unwrap().join("task.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        stored["title"],
        serde_json::json!({ "value": "Agent title", "source": "agent" })
    );
    assert!(stored.get("agent_title").is_none());
    assert_eq!(store.read_task("task-title").unwrap().title, task.title);
}

#[test]
fn blocked_store_open_does_not_create_product_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let runtime_dir = dir.path().join(".openaide-runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let _lock =
        crate::storage_runtime::RuntimeLock::acquire(runtime_dir.join("storage-writer.lock"))
            .unwrap();

    assert!(matches!(
        Store::open(dir.path().to_path_buf()),
        Err(StoreOpenError::LockedByLiveServer)
    ));
    assert!(!dir.path().join("tasks").exists());
    assert!(!dir.path().join("diagnostics").exists());
}

#[test]
fn diagnostics_include_redacted_active_task_session_state() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let mut active = task_record(
        "task-active",
        TaskStatus::Active,
        "2026-07-06T10:00:00.000Z",
    );
    active.agent_id = "codex".to_string();
    active.active_turn_id = Some("turn-1".to_string());
    active.agent_session_id = Some("native-session-secret".to_string());
    store.write_task(&active).unwrap();
    store
        .write_task(&task_record(
            "task-idle",
            TaskStatus::Inactive,
            "2026-07-06T09:00:00.000Z",
        ))
        .unwrap();

    let diagnostics = crate::tasks::query_store::TaskReadStore::new(store)
        .diagnostics(7)
        .unwrap();

    assert_eq!(diagnostics.active_count, 1);
    assert_eq!(diagnostics.active_tasks.len(), 1);
    let active = &diagnostics.active_tasks[0];
    assert_eq!(active.task_id, "task-active");
    assert_eq!(active.agent_id, "codex");
    assert_eq!(active.status, TaskStatus::Active);
    assert_eq!(active.active_turn_id.as_deref(), Some("turn-1"));
    assert!(active.has_agent_session);
}

#[test]
fn dropping_store_releases_writer_guard() {
    let dir = tempfile::tempdir().unwrap();
    let first = Store::open(dir.path().to_path_buf()).unwrap();
    drop(first);

    let second = Store::open(dir.path().to_path_buf()).unwrap();

    assert_eq!(
        second.recovery_classification(),
        RecoveryClassification::UncleanPreviousShutdown
    );
}

#[test]
fn cloned_store_keeps_writer_guard_until_last_clone_drops() {
    let dir = tempfile::tempdir().unwrap();
    let first = Store::open(dir.path().to_path_buf()).unwrap();
    let clone = first.clone();
    drop(first);

    assert!(matches!(
        Store::open(dir.path().to_path_buf()),
        Err(StoreOpenError::LockedByLiveServer)
    ));
    drop(clone);
    assert!(Store::open(dir.path().to_path_buf()).is_ok());
}

#[test]
fn open_store_classifies_unclean_previous_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let marker = runtime_marker_path(dir.path());
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, br#"{"schemaVersion":1,"state":"open"}"#).unwrap();

    let store = Store::open(dir.path().to_path_buf()).unwrap();

    assert_eq!(
        store.recovery_classification(),
        RecoveryClassification::UncleanPreviousShutdown
    );
}

#[test]
fn clean_shutdown_writes_clean_marker() {
    let dir = tempfile::tempdir().unwrap();
    let marker = runtime_marker_path(dir.path());
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    store.mark_clean_shutdown().unwrap();
    drop(store);

    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(marker).unwrap()).unwrap();

    assert_eq!(marker["state"], "clean");
}

#[test]
fn persisted_tool_artifacts_keep_a_lightweight_file_summary() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let mut message = NormalizedMessage::Activity {
        id: "activity_1".to_string(),
        title: "Updated file".to_string(),
        status: ActivityStatus::Completed,
        created_at: "2026-07-02T00:00:00Z".to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Tool {
            tool_call_id: None,
            name: "edit".to_string(),
            status: ActivityStatus::Completed,
            input_summary: None,
            output_preview: None,
            detail_artifact_id: None,
            details: Some(Box::new(ActivityToolDetails {
                locations: Vec::new(),
                content: vec![ActivityToolContent::Diff {
                    path: "/workspace/src/chat.ts".to_string(),
                    old_text: Some("old".to_string()),
                    new_text: "new".to_string(),
                }],
                input: None,
                output: None,
            })),
        }],
    };

    store
        .persist_tool_artifacts("task_1", &mut message)
        .unwrap();

    let NormalizedMessage::Activity { steps, .. } = message else {
        panic!("expected activity");
    };
    let ActivityStep::Tool {
        input_summary,
        detail_artifact_id,
        details,
        ..
    } = &steps[0]
    else {
        panic!("expected tool step");
    };
    assert_eq!(input_summary.as_deref(), Some("chat.ts"));
    assert_eq!(detail_artifact_id.as_deref(), Some("activity_1_0"));
    assert!(details.is_none());
}

#[test]
fn pages_hydrate_missing_tool_file_summaries_from_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let task_id = "task_tool_summary";
    std::fs::create_dir_all(store.task_dir(task_id).unwrap()).unwrap();
    let mut message = activity_message_with_edit_details("activity_1");
    store
        .persist_tool_artifacts(task_id, &mut message.message)
        .unwrap();
    let NormalizedMessage::Activity { steps, .. } = &mut message.message else {
        panic!("expected activity");
    };
    let ActivityStep::Tool { input_summary, .. } = &mut steps[0] else {
        panic!("expected tool step");
    };
    *input_summary = None;
    write_stored_messages(
        &store,
        task_id,
        &[StoredMessage {
            sequence: 1,
            chat: message,
        }],
    );

    let page = store.tail_page(task_id, 1).unwrap();

    let NormalizedMessage::Activity { steps, .. } = &page.items[0].message else {
        panic!("expected activity");
    };
    let ActivityStep::Tool { input_summary, .. } = &steps[0] else {
        panic!("expected tool step");
    };
    assert_eq!(input_summary.as_deref(), Some("chat.ts"));
}

#[test]
fn pages_before_legacy_message_cursor_returned_to_frontend() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let task_id = "task_legacy_cursor";
    std::fs::create_dir_all(store.task_dir(task_id).unwrap()).unwrap();
    store
        .append_message(task_id, chat_message("legacy-1", "First"))
        .unwrap();
    store
        .append_message(task_id, chat_message("legacy-2", "Second"))
        .unwrap();
    store
        .append_message(task_id, chat_message("legacy-3", "Third"))
        .unwrap();

    let tail = store.tail_page(task_id, 2).unwrap();
    assert_eq!(tail.start_cursor.as_deref(), Some("legacy-1"));

    let page = store.page_before(task_id, "legacy-2", 1).unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].message_id, "legacy-1");
    assert_eq!(page.start_cursor.as_deref(), Some("legacy-1"));
    assert!(!page.has_before);
}

#[test]
fn tail_page_keeps_the_user_prompt_before_a_large_activity_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let task_id = "task_large_activity_run";
    std::fs::create_dir_all(store.task_dir(task_id).unwrap()).unwrap();
    let mut messages = vec![StoredMessage {
        sequence: 1,
        chat: agent_chat_message("older", "Older response"),
    }];
    messages.push(StoredMessage {
        sequence: 2,
        chat: chat_message("prompt", "Investigate this"),
    });
    for sequence in 3..=102 {
        messages.push(StoredMessage {
            sequence,
            chat: activity_message_with_edit_details(&format!("tool-{sequence}")),
        });
    }
    write_stored_messages(&store, task_id, &messages);

    let page = store.tail_page(task_id, 100).unwrap();

    assert_eq!(page.items.len(), 101);
    assert!(matches!(
        page.items[0].message,
        NormalizedMessage::User { .. }
    ));
    assert_eq!(page.items[0].message_id, "prompt");
    assert!(page.has_before);
}

#[test]
fn tail_page_targets_recent_conversation_turns_instead_of_raw_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let task_id = "task_recent_turns";
    std::fs::create_dir_all(store.task_dir(task_id).unwrap()).unwrap();
    let messages: Vec<_> = (1..=12)
        .map(|sequence| StoredMessage {
            sequence,
            chat: chat_message(&format!("prompt-{sequence}"), "Prompt"),
        })
        .collect();
    write_stored_messages(&store, task_id, &messages);

    let page = store.tail_page(task_id, 1).unwrap();

    assert_eq!(page.items.len(), 10);
    assert_eq!(page.items[0].message_id, "prompt-3");
    assert!(page.has_before);
}

#[test]
fn task_navigation_orders_active_tasks_by_last_activity() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let mut earlier = task_record(
        "task_earlier",
        TaskStatus::Active,
        "2026-07-02T10:00:00.000Z",
    );
    let later = task_record("task_later", TaskStatus::Active, "2026-07-02T10:05:00.000Z");
    store.write_task(&earlier).unwrap();
    store.write_task(&later).unwrap();

    assert_eq!(
        listed_task_ids(&store),
        ["task_later".to_string(), "task_earlier".to_string()],
    );

    earlier.updated_at = "2026-07-02T10:10:00.000Z".to_string();
    store.write_task(&earlier).unwrap();

    assert_eq!(
        listed_task_ids(&store),
        ["task_later".to_string(), "task_earlier".to_string()],
    );

    earlier.last_activity = "2026-07-02T10:10:00.000Z".to_string();
    earlier.updated_at = earlier.last_activity.clone();
    store.write_task(&earlier).unwrap();

    assert_eq!(
        listed_task_ids(&store),
        ["task_earlier".to_string(), "task_later".to_string()],
    );
}

#[test]
fn plain_drop_preserves_unclean_marker() {
    let dir = tempfile::tempdir().unwrap();
    let marker = runtime_marker_path(dir.path());
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    drop(store);

    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(marker).unwrap()).unwrap();

    assert_eq!(marker["state"], "open");
}

#[test]
fn schema_mismatch_returns_structured_error() {
    let dir = tempfile::tempdir().unwrap();
    let marker = runtime_marker_path(dir.path());
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    let marker_json = serde_json::json!({
        "schemaVersion": 999_u32,
        "state": "clean"
    });
    std::fs::write(&marker, serde_json::to_vec(&marker_json).unwrap()).unwrap();

    assert!(matches!(
        Store::open(dir.path().to_path_buf()),
        Err(StoreOpenError::IncompatibleSchema { found: 999 })
    ));
}

fn runtime_marker_path(root: &Path) -> PathBuf {
    root.join(".openaide-runtime").join("storage-state.json")
}

fn write_stored_messages(store: &Store, task_id: &str, messages: &[StoredMessage]) {
    let mut bytes = Vec::new();
    for message in messages {
        serde_json::to_writer(&mut bytes, message).unwrap();
        bytes.push(b'\n');
    }
    std::fs::write(
        store.task_dir(task_id).unwrap().join("messages.jsonl"),
        bytes,
    )
    .unwrap();
}

fn chat_message(id: &str, text: &str) -> ChatMessage {
    ChatMessage {
        cursor: id.to_string(),
        identity: id.to_string(),
        message_type: "user".to_string(),
        message_id: id.to_string(),
        message: NormalizedMessage::User {
            id: id.to_string(),
            text: text.to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            attachments: Vec::new(),
        },
    }
}

fn agent_chat_message(id: &str, text: &str) -> ChatMessage {
    ChatMessage {
        cursor: id.to_string(),
        identity: id.to_string(),
        message_type: "agent_text".to_string(),
        message_id: id.to_string(),
        message: NormalizedMessage::AgentText {
            id: id.to_string(),
            text: text.to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            streaming: false,
        },
    }
}

fn activity_message_with_edit_details(id: &str) -> ChatMessage {
    ChatMessage {
        cursor: id.to_string(),
        identity: id.to_string(),
        message_type: "activity".to_string(),
        message_id: id.to_string(),
        message: NormalizedMessage::Activity {
            id: id.to_string(),
            title: "Updated file".to_string(),
            status: ActivityStatus::Completed,
            created_at: "2026-07-02T00:00:00Z".to_string(),
            collapsed: true,
            steps: vec![ActivityStep::Tool {
                tool_call_id: None,
                name: "edit".to_string(),
                status: ActivityStatus::Completed,
                input_summary: None,
                output_preview: None,
                detail_artifact_id: None,
                details: Some(Box::new(ActivityToolDetails {
                    locations: Vec::new(),
                    content: vec![ActivityToolContent::Diff {
                        path: "/workspace/src/chat.ts".to_string(),
                        old_text: Some("old".to_string()),
                        new_text: "new".to_string(),
                    }],
                    input: None,
                    output: None,
                })),
            }],
        },
    }
}

#[test]
fn visible_task_queries_exclude_client_private_new_tasks() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let visible = task_record("task-visible", TaskStatus::Inactive, "1");
    let mut new_task = task_record("task-new", TaskStatus::Inactive, "2");
    new_task.lifecycle = super::records::TaskLifecycle::New {
        owner_client_instance_id: openaide_app_server_protocol::ids::ClientInstanceId::from(
            "client-a",
        ),
    };
    store.write_task(&visible).unwrap();
    store.write_task(&new_task).unwrap();

    assert_eq!(listed_task_ids(&store), vec!["task-visible"]);
    assert_eq!(store.list_all_task_records().unwrap().len(), 2);
}

fn listed_task_ids(store: &Store) -> Vec<String> {
    store
        .list_tasks()
        .unwrap()
        .into_iter()
        .map(|record| record.task_id)
        .collect()
}

#[test]
fn local_history_timestamp_advances_for_every_chat_write() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let task_id = "task-history-clock";

    store
        .append_message(task_id, chat_message("message-1", "First"))
        .unwrap();
    let first = store.local_history_updated_at(task_id).unwrap();
    store
        .append_message(task_id, chat_message("message-2", "Second"))
        .unwrap();
    let second = store.local_history_updated_at(task_id).unwrap();

    assert!(second > first);
}

#[test]
fn native_history_replacement_records_the_native_history_clock() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let task_id = "task-native-history-clock";
    let native_updated_at = u128::MAX - 1;

    store
        .replace_messages_with_normalized_at(
            task_id,
            vec![NormalizedMessage::AgentText {
                id: "native-message".to_string(),
                text: "Loaded history".to_string(),
                created_at: "1".to_string(),
                streaming: false,
            }],
            native_updated_at,
        )
        .unwrap();

    assert_eq!(
        store.local_history_updated_at(task_id).unwrap(),
        native_updated_at.to_string()
    );
}

fn task_record(task_id: &str, status: TaskStatus, created_at: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitle::new(
            task_id,
            crate::storage::records::TaskTitleSource::User,
        ),
        status,
        task_version: 1,
        message_history_version: 1,
        unread: false,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
        last_activity: created_at.to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace".to_string(),
        lifecycle: super::records::TaskLifecycle::Visible,
        agent_session_id: None,
        active_turn_id: None,
        archived: false,
        tombstoned: false,
        revision: 1,
        config_options: HashMap::new(),
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        preparation: TaskPreparationRecord::Ready,
    }
}
