use super::*;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, ActivityToolContent, ActivityToolDetails, AgentMessagePart,
    AgentMessageRole, ChatMessage, ConfigOption, ConfigOptionCurrentValue, ConfigOptionKind,
    ConfigOptionsCatalog, ConfigOptionsStatus, IsolationKind, NormalizedMessage, TaskStatus,
};
use crate::storage::records::{StoredMessage, TaskPreparationRecord, TaskRecord};
use crate::storage_runtime::RecoveryClassification;
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
fn task_title_persists_automatic_title_state() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let mut task = task_record("task-title", TaskStatus::Inactive, "1");
    task.title = crate::storage::records::TaskTitleState::from_title(
        crate::storage::records::TaskTitle::new(
            "  Agent title  ",
            crate::storage::records::TaskTitleSource::Agent,
        ),
    );

    store.write_task(&task).unwrap();

    let stored = serde_json::to_value(store.read_task("task-title").unwrap()).unwrap();
    assert_eq!(
        stored["title"],
        serde_json::json!({
            "ownership": "automatic",
            "title": { "value": "Agent title", "source": "agent" }
        })
    );
    assert!(stored.get("agent_title").is_none());
    assert_eq!(store.read_task("task-title").unwrap().title, task.title);
}

#[test]
fn legacy_single_task_titles_migrate_without_changing_the_visible_owner() {
    let task = task_record("task-legacy-title", TaskStatus::Inactive, "1");
    let mut persisted = serde_json::to_value(task).unwrap();
    persisted["title"] = serde_json::json!({ "value": "Legacy automatic", "source": "agent" });

    let automatic: TaskRecord = serde_json::from_value(persisted.clone()).unwrap();
    assert_eq!(
        automatic.title.effective().map(|title| title.value()),
        Some("Legacy automatic")
    );
    assert!(!automatic.title.has_user_override());

    persisted["title"] = serde_json::json!({ "value": "Legacy user", "source": "user" });
    let user: TaskRecord = serde_json::from_value(persisted).unwrap();
    assert_eq!(
        user.title.effective().map(|title| title.value()),
        Some("Legacy user")
    );
    assert!(user.title.has_user_override());
}

#[test]
fn task_lifecycle_exclusively_owns_open_and_archived_list_membership() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let open = task_record("task-open", TaskStatus::Inactive, "2");
    let mut archived = task_record("task-archived", TaskStatus::Inactive, "1");
    archived.lifecycle = crate::storage::records::TaskLifecycle::Archived;

    store.write_task(&open).unwrap();
    store.write_task(&archived).unwrap();

    assert_eq!(store.list_tasks().unwrap()[0].task_id, "task-open");
    assert_eq!(
        store.list_archived_tasks().unwrap()[0].task_id,
        "task-archived"
    );
    let persisted = serde_json::to_value(store.read_task("task-archived").unwrap()).unwrap();
    assert_eq!(
        persisted["lifecycle"]["state"],
        serde_json::json!("archived")
    );
    assert!(persisted.get("archived").is_none());
}

#[test]
fn legacy_archived_flag_migrates_into_the_archived_lifecycle() {
    let task = task_record("task-legacy-archived", TaskStatus::Inactive, "1");
    let mut persisted = serde_json::to_value(task).unwrap();
    persisted["lifecycle"]["state"] = serde_json::json!("visible");
    persisted["archived"] = serde_json::json!(true);

    let loaded: TaskRecord = serde_json::from_value(persisted).unwrap();

    assert_eq!(
        loaded.lifecycle,
        crate::storage::records::TaskLifecycle::Archived
    );
    assert!(serde_json::to_value(loaded)
        .unwrap()
        .get("archived")
        .is_none());
}

#[test]
fn legacy_select_config_option_without_kind_remains_readable() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let mut task = task_record("task-legacy-config", TaskStatus::Inactive, "1");
    task.config_options_catalog = Some(ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: ConfigOptionsStatus::Ready,
        options: vec![ConfigOption {
            id: "model".to_string(),
            label: "Model".to_string(),
            description: None,
            category: None,
            kind: ConfigOptionKind::Select,
            current_value: ConfigOptionCurrentValue::id("gpt-5"),
            values: Vec::new(),
        }],
    });
    store.write_task(&task).unwrap();

    let mut persisted = serde_json::to_value(&task).unwrap();
    persisted["config_options_catalog"]["options"][0]
        .as_object_mut()
        .unwrap()
        .remove("kind");
    let loaded: TaskRecord = serde_json::from_value(persisted).unwrap();

    assert_eq!(
        loaded.config_options_catalog.unwrap().options[0].kind,
        ConfigOptionKind::Select
    );
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
fn successful_journal_start_removes_only_unsupported_legacy_task_storage() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_task = dir.path().join("tasks/task-old");
    std::fs::create_dir_all(legacy_task.join("tool-artifacts")).unwrap();
    std::fs::write(legacy_task.join("task.json"), b"legacy task").unwrap();
    std::fs::write(legacy_task.join("messages.jsonl"), b"legacy chat").unwrap();
    std::fs::write(
        legacy_task.join("tool-artifacts/tool-old.json"),
        b"legacy tool",
    )
    .unwrap();
    let agent_sentinel = dir.path().join("agents/catalog.json");
    std::fs::create_dir_all(agent_sentinel.parent().unwrap()).unwrap();
    std::fs::write(&agent_sentinel, b"preserve agent state").unwrap();

    let _store = Store::open(dir.path().to_path_buf()).unwrap();

    assert!(!dir.path().join("tasks").exists());
    assert_eq!(
        std::fs::read(agent_sentinel).unwrap(),
        b"preserve agent state"
    );
    assert!(dir.path().join("task-store-v1/tasks").is_dir());
}

#[cfg(unix)]
#[test]
fn legacy_cleanup_failure_does_not_block_journal_start() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let legacy_tasks = dir.path().join("tasks");
    std::fs::create_dir_all(legacy_tasks.join("task-old")).unwrap();
    std::fs::write(legacy_tasks.join("task-old/task.json"), b"legacy task").unwrap();
    std::fs::set_permissions(&legacy_tasks, std::fs::Permissions::from_mode(0o000)).unwrap();

    let opened = Store::open(dir.path().to_path_buf());

    assert!(
        opened.is_ok(),
        "obsolete-file permissions must not block startup"
    );
    assert!(legacy_tasks.exists(), "failed cleanup is retried later");
    std::fs::set_permissions(&legacy_tasks, std::fs::Permissions::from_mode(0o700)).unwrap();
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
    ensure_task(&store, "task_1");
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
            permission_outcomes: Vec::new(),
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
    assert_eq!(
        detail_artifact_id.as_deref(),
        Some(crate::storage::tool_artifacts::tool_artifact_id("activity_1", 0).as_str())
    );
    assert!(details.is_none());
}

#[test]
fn pages_hydrate_missing_tool_file_summaries_from_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let task_id = "task_tool_summary";
    ensure_task(&store, task_id);
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
    ensure_task(&store, task_id);
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
    ensure_task(&store, task_id);
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
    ensure_task(&store, task_id);
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
    ensure_task(store, task_id);
    let projection = store.task_journal().load(task_id).unwrap();
    store
        .replace_projection_messages(projection, messages.to_vec(), 0)
        .unwrap();
}

fn ensure_task(store: &Store, task_id: &str) {
    if store.read_task(task_id).is_err() {
        store
            .write_task(&task_record(task_id, TaskStatus::Inactive, "1"))
            .unwrap();
    }
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
        message_type: "agent_message".to_string(),
        message_id: id.to_string(),
        message: NormalizedMessage::AgentMessage {
            id: id.to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: text.to_string(),
            }],
            created_at: "2026-07-01T00:00:00Z".to_string(),
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
                permission_outcomes: Vec::new(),
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
    new_task.lifecycle = super::records::TaskLifecycle::Prepared {
        lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
            "client-a",
        )),
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
    ensure_task(&store, task_id);

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
fn agent_text_chunk_is_durable_without_rewriting_existing_history() {
    let temp = tempfile::tempdir().unwrap();
    let task_id = "task-agent-text-journal";
    let messages_path = temp
        .path()
        .join("tasks")
        .join(task_id)
        .join("messages.jsonl");
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        ensure_task(&store, task_id);
        store
            .append_message(task_id, agent_chat_message("agent-message", "Hello"))
            .unwrap();
        assert!(!messages_path.exists());

        store
            .append_agent_message_part(
                task_id,
                NormalizedMessage::AgentMessage {
                    id: "agent-message".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: " world".to_string(),
                    }],
                    created_at: "2026-07-01T00:00:01Z".to_string(),
                },
            )
            .unwrap();

        assert!(!messages_path.exists());
    }

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let page = reopened.tail_page(task_id, 10).unwrap();
    let NormalizedMessage::AgentMessage { parts, .. } = &page.items[0].message else {
        panic!("expected Agent message");
    };
    assert_eq!(
        parts,
        &[AgentMessagePart::Text {
            text: "Hello world".to_string(),
        }]
    );
}

#[test]
fn first_agent_text_chunk_appends_without_rewriting_existing_history() {
    let temp = tempfile::tempdir().unwrap();
    let task_id = "task-first-agent-text-journal";
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    ensure_task(&store, task_id);
    store
        .append_message(task_id, chat_message("user-message", "Prompt"))
        .unwrap();
    let messages_path = store.task_dir(task_id).unwrap().join("messages.jsonl");
    assert!(!messages_path.exists());

    let result = store
        .append_agent_message_part(
            task_id,
            NormalizedMessage::AgentMessage {
                id: "agent-message".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: "Answer".to_string(),
                }],
                created_at: "2026-07-01T00:00:01Z".to_string(),
            },
        )
        .unwrap();

    assert!(matches!(
        result,
        crate::storage::message_store::AgentMessageAppend::Appended(_)
    ));
    assert!(!messages_path.exists());
    assert_eq!(store.read_messages(task_id).unwrap().len(), 2);
}

#[test]
fn compacted_agent_text_accepts_later_durable_chunks() {
    let temp = tempfile::tempdir().unwrap();
    let task_id = "task-agent-text-compaction";
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    ensure_task(&store, task_id);
    store
        .append_message(task_id, agent_chat_message("agent-message", "one"))
        .unwrap();
    for text in [" two", " three"] {
        store
            .append_agent_message_part(
                task_id,
                NormalizedMessage::AgentMessage {
                    id: "agent-message".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: text.to_string(),
                    }],
                    created_at: "2026-07-01T00:00:01Z".to_string(),
                },
            )
            .unwrap();
    }

    store.compact_message_journal(task_id).unwrap();
    assert!(!store
        .task_dir(task_id)
        .unwrap()
        .join("message_journal.jsonl")
        .exists());
    store
        .append_agent_message_part(
            task_id,
            NormalizedMessage::AgentMessage {
                id: "agent-message".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: " four".to_string(),
                }],
                created_at: "2026-07-01T00:00:02Z".to_string(),
            },
        )
        .unwrap();
    drop(store);

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let messages = reopened.read_messages(task_id).unwrap();
    let NormalizedMessage::AgentMessage { parts, .. } = &messages[0].chat.message else {
        panic!("expected Agent message");
    };
    assert_eq!(
        parts,
        &[AgentMessagePart::Text {
            text: "one two three four".to_string(),
        }]
    );
}

#[test]
fn native_history_replacement_records_the_native_history_clock() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let task_id = "task-native-history-clock";
    ensure_task(&store, task_id);
    let native_updated_at = u128::MAX - 1;

    store
        .replace_messages_with_normalized_at(
            task_id,
            vec![NormalizedMessage::AgentMessage {
                id: "native-message".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: "Loaded history".to_string(),
                }],
                created_at: "1".to_string(),
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
        title: crate::storage::records::TaskTitleState::from_title(
            crate::storage::records::TaskTitle::new(
                task_id,
                crate::storage::records::TaskTitleSource::User,
            ),
        ),
        status,
        task_version: 1,
        message_history_version: 1,
        unread: false,
        attention: None,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
        last_activity: created_at.to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: super::records::TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
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
