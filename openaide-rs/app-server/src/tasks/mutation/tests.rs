use std::sync::{Arc, Mutex};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{IsolationKind, NormalizedMessage, TaskStatus};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::send_receipts::TaskSendReceipt;
use crate::storage::Store;
use crate::task_events::{TaskUpdateNotifier, TaskUpdateReceiver};
use crate::tasks::mutation::{
    TaskCommitOptions, TaskCommitOutcome, TaskCommitRejection, TaskMutationResult, TaskMutations,
};
use crate::tasks::runtime_state::RuntimeState;

#[test]
fn metadata_commit_assigns_revision_once_and_returns_publication_facts() {
    let (_dir, store, mutations, notifications) = test_mutations(0);
    let record = task_record("task_commit");
    store.write_task(&record).unwrap();

    let result = mutations
        .commit_existing_task("task_commit", TaskCommitOptions::metadata(), |ctx| {
            ctx.task_mut().unread = false;
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();

    let TaskCommitOutcome::Committed(facts) = result.outcome else {
        panic!("metadata commit should be committed");
    };
    assert_eq!(facts.task_id, "task_commit");
    assert_eq!(facts.revision, 1);

    let stored = store.read_task("task_commit").unwrap();
    assert!(!stored.unread);
    assert_eq!(stored.task_version, 1);
    assert_eq!(stored.revision, 1);
    assert_eq!(mutations.current_revision(), 1);

    let notification = notifications.try_recv().unwrap();
    assert_eq!(notification.task_id, "task_commit");
    assert_eq!(notification.revision, 1);
}

#[test]
fn unchanged_commit_returns_rejection_without_revision_or_notification() {
    let (_dir, store, mutations, notifications) = test_mutations(5);
    let mut record = task_record("task_no_change");
    record.revision = 5;
    record.message_history_version = 7;
    record.task_version = 3;
    store.write_task(&record).unwrap();

    let result = mutations
        .commit_existing_task("task_no_change", TaskCommitOptions::metadata(), |_ctx| {
            Ok(TaskMutationResult::Unchanged)
        })
        .unwrap();

    assert_rejected_no_change(result.outcome);
    assert_task_unchanged(&store.read_task("task_no_change").unwrap(), &record);
    assert_eq!(mutations.current_revision(), 5);
    assert!(notifications.try_recv().is_err());
}

#[test]
fn rejected_commit_returns_rejection_without_storage_or_publication_facts() {
    let (_dir, store, mutations, notifications) = test_mutations(2);
    let mut record = task_record("task_rejected");
    record.revision = 2;
    record.message_history_version = 4;
    record.task_version = 9;
    store.write_task(&record).unwrap();

    let result = mutations
        .commit_existing_task("task_rejected", TaskCommitOptions::metadata(), |ctx| {
            ctx.task_mut().title = "Should not persist".to_string();
            Ok(TaskMutationResult::Rejected)
        })
        .unwrap();

    assert_rejected_no_change(result.outcome);
    assert_task_unchanged(&store.read_task("task_rejected").unwrap(), &record);
    assert_eq!(mutations.current_revision(), 2);
    assert!(notifications.try_recv().is_err());
}

#[test]
fn chat_commit_refreshes_message_history_before_task_write() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    let record = task_record("task_chat_refresh");
    store.write_task(&record).unwrap();
    mutations
        .append_message(
            "task_chat_refresh",
            NormalizedMessage::User {
                id: "message_1".to_string(),
                text: "hello".to_string(),
                created_at: "2".to_string(),
                attachments: Vec::new(),
            },
        )
        .unwrap();

    let result = mutations
        .commit_existing_task(
            "task_chat_refresh",
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                ctx.task_mut().title = "Updated".to_string();
                Ok(TaskMutationResult::Changed)
            },
        )
        .unwrap();

    let TaskCommitOutcome::Committed(facts) = result.outcome else {
        panic!("chat commit should be committed");
    };
    assert_eq!(facts.revision, 1);
    assert_eq!(
        store
            .read_task("task_chat_refresh")
            .unwrap()
            .message_history_version,
        1
    );
}

#[test]
fn commit_rejects_task_identity_mutation_without_advancing_global_revision() {
    let (_dir, store, mutations, notifications) = test_mutations(8);
    let mut record = task_record("task_identity_mutation");
    record.revision = 8;
    store.write_task(&record).unwrap();

    let error = mutations
        .commit_existing_task(
            "task_identity_mutation",
            TaskCommitOptions::metadata(),
            |ctx| {
                ctx.task_mut().task_id = "bad/task/id".to_string();
                Ok(TaskMutationResult::Changed)
            },
        )
        .unwrap_err();

    assert!(error.to_string().contains("task identity"));
    assert_eq!(mutations.current_revision(), 8);
    assert_eq!(
        store.read_task("task_identity_mutation").unwrap().revision,
        8
    );
    assert!(notifications.try_recv().is_err());
}

#[test]
fn rejected_commit_rolls_back_context_message_side_effects() {
    let (_dir, store, mutations, notifications) = test_mutations(3);
    let mut record = task_record("task_reject_side_effect");
    record.revision = 3;
    store.write_task(&record).unwrap();
    mutations
        .append_message(
            "task_reject_side_effect",
            NormalizedMessage::User {
                id: "message_1".to_string(),
                text: "original".to_string(),
                created_at: "2".to_string(),
                attachments: Vec::new(),
            },
        )
        .unwrap();
    let original_messages = store.read_messages("task_reject_side_effect").unwrap();

    let result = mutations
        .commit_existing_task(
            "task_reject_side_effect",
            TaskCommitOptions::metadata(),
            |ctx| {
                ctx.append_message(NormalizedMessage::AgentText {
                    id: "message_2".to_string(),
                    text: "should roll back".to_string(),
                    created_at: "3".to_string(),
                    streaming: false,
                })?;
                Ok(TaskMutationResult::Rejected)
            },
        )
        .unwrap();

    assert_rejected_no_change(result.outcome);
    assert_eq!(
        serde_json::to_value(store.read_messages("task_reject_side_effect").unwrap()).unwrap(),
        serde_json::to_value(original_messages).unwrap()
    );
    assert_eq!(
        store.read_task("task_reject_side_effect").unwrap().revision,
        3
    );
    assert_eq!(mutations.current_revision(), 3);
    assert!(notifications.try_recv().is_err());
}

#[test]
fn rejected_commit_rolls_back_send_receipt_side_effects() {
    let (_dir, store, mutations, notifications) = test_mutations(3);
    let mut record = task_record("task_reject_receipt");
    record.revision = 3;
    store.write_task(&record).unwrap();
    store
        .write_send_receipt(
            "task_reject_receipt",
            send_receipt("accepted-send", "accepted-message"),
        )
        .unwrap();
    let mutation_store = store.clone();

    let result = mutations
        .commit_existing_task(
            "task_reject_receipt",
            TaskCommitOptions::metadata(),
            |_ctx| {
                mutation_store.write_send_receipt(
                    "task_reject_receipt",
                    send_receipt("rejected-send", "rejected-message"),
                )?;
                Ok(TaskMutationResult::Rejected)
            },
        )
        .unwrap();

    assert_rejected_no_change(result.outcome);
    assert!(store
        .read_send_receipt("task_reject_receipt", "accepted-send")
        .unwrap()
        .is_some());
    assert!(store
        .read_send_receipt("task_reject_receipt", "rejected-send")
        .unwrap()
        .is_none());
    assert_eq!(mutations.current_revision(), 3);
    assert!(notifications.try_recv().is_err());
}

#[test]
fn invariant_failure_rolls_back_context_message_side_effects() {
    let (_dir, store, mutations, notifications) = test_mutations(4);
    let mut record = task_record("task_invariant_side_effect");
    record.revision = 4;
    store.write_task(&record).unwrap();

    let error = mutations
        .commit_existing_task(
            "task_invariant_side_effect",
            TaskCommitOptions::metadata(),
            |ctx| {
                ctx.append_message(NormalizedMessage::AgentText {
                    id: "message_1".to_string(),
                    text: "should roll back".to_string(),
                    created_at: "3".to_string(),
                    streaming: false,
                })?;
                ctx.task_mut().revision = 99;
                Ok(TaskMutationResult::Changed)
            },
        )
        .unwrap_err();

    assert!(error.to_string().contains("version fields"));
    assert!(store
        .read_messages("task_invariant_side_effect")
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .read_task("task_invariant_side_effect")
            .unwrap()
            .revision,
        4
    );
    assert_eq!(mutations.current_revision(), 4);
    assert!(notifications.try_recv().is_err());
}

#[test]
fn create_task_persists_initial_chat_and_returns_commit_facts() {
    let (_dir, store, mutations, notifications) = test_mutations(10);
    let mut record = task_record("task_create_commit");
    record.task_version = 1;

    let result = mutations
        .create_task(
            record,
            vec![NormalizedMessage::User {
                id: "message_1".to_string(),
                text: "hello".to_string(),
                created_at: "2".to_string(),
                attachments: Vec::new(),
            }],
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: Some(100),
            },
        )
        .unwrap();

    let TaskCommitOutcome::Committed(facts) = result.outcome else {
        panic!("create commit should be committed");
    };
    assert_eq!(facts.task_id, "task_create_commit");
    assert_eq!(facts.revision, 11);
    assert!(result.response_snapshot.is_some());

    let stored = store.read_task("task_create_commit").unwrap();
    assert_eq!(stored.task_version, 1);
    assert_eq!(stored.revision, 11);
    assert_eq!(stored.message_history_version, 1);
    assert_eq!(store.read_messages("task_create_commit").unwrap().len(), 1);
    assert_eq!(mutations.current_revision(), 11);

    let notification = notifications.try_recv().unwrap();
    assert_eq!(notification.task_id, "task_create_commit");
    assert_eq!(notification.revision, 11);
}

#[test]
fn create_task_persists_initial_history_in_one_message_batch() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    let messages = ["first", "second", "third"]
        .into_iter()
        .enumerate()
        .map(|(index, text)| NormalizedMessage::User {
            id: format!("message_{}", index + 1),
            text: text.to_string(),
            created_at: (index + 2).to_string(),
            attachments: Vec::new(),
        })
        .collect();

    mutations
        .create_task(
            task_record("task_bulk_history"),
            messages,
            TaskCommitOptions::metadata(),
        )
        .unwrap();

    assert_eq!(store.message_file_write_count_for_test(), 1);
    assert_eq!(store.read_messages("task_bulk_history").unwrap().len(), 3);
}

#[test]
fn failed_create_task_write_rolls_back_initial_chat_and_revision() {
    let (_dir, store, mutations, notifications) = test_mutations(6);
    let mut record = task_record("task_create_write_failure");
    record.task_version = 1;

    let error = mutations
        .create_task_with_validation_and_writer(
            record,
            vec![NormalizedMessage::User {
                id: "message_1".to_string(),
                text: "hello".to_string(),
                created_at: "2".to_string(),
                attachments: Vec::new(),
            }],
            TaskCommitOptions::metadata(),
            |_| Ok(()),
            |_store, _task| Err(RuntimeError::Storage("forced write failure".to_string())),
        )
        .unwrap_err();

    assert!(error.to_string().contains("forced write failure"));
    assert_eq!(mutations.current_revision(), 6);
    assert!(store.read_task("task_create_write_failure").is_err());
    assert!(store
        .read_messages("task_create_write_failure")
        .unwrap()
        .is_empty());
    assert!(notifications.try_recv().is_err());
}

#[test]
fn migrated_service_paths_have_no_direct_task_updated_calls() {
    let allowed = [("src/tasks/mutation/commit.rs", "notify_task_updated")];
    let mut actual = Vec::new();

    for path in rust_source_files("src/tasks") {
        let path_string = path.to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&path).unwrap();
        actual.extend(task_updated_calls_by_function(&path_string, &text));
    }

    actual.sort();
    let mut expected: Vec<_> = allowed
        .into_iter()
        .map(|(path, function)| (path.to_string(), function.to_string()))
        .collect();
    expected.sort();
    assert_eq!(actual, expected);
}

#[test]
fn task_turn_lifecycle_has_no_direct_commit_bypasses() {
    let mut offenders = Vec::new();
    let lifecycle_paths = rust_source_files("src/tasks/turn_lifecycle")
        .into_iter()
        .chain([std::path::PathBuf::from("src/tasks/turn_lifecycle.rs")]);

    for path in lifecycle_paths {
        let path_string = path.to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&path).unwrap();
        for (line_index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            for pattern in [
                ".task_updated(",
                " task_updated(",
                "next_revision(",
                ".write_task(",
                "append_normalized_to_store(",
            ] {
                if trimmed.contains(pattern) {
                    offenders.push(format!(
                        "{}:{} contains {}",
                        path_string,
                        line_index + 1,
                        pattern
                    ));
                }
            }
        }
    }

    let create_path = "src/tasks/turn_lifecycle/create.rs";
    let create_text = std::fs::read_to_string(create_path).unwrap();
    for (line_index, line) in create_text.lines().enumerate() {
        let trimmed = line.trim_start();
        for pattern in ["build_snapshot(", "self.snapshot("] {
            if trimmed.contains(pattern) {
                offenders.push(format!(
                    "{}:{} contains {}",
                    create_path,
                    line_index + 1,
                    pattern
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "TaskTurnLifecycle must route durable Task commits through TaskMutations:\n{}",
        offenders.join("\n")
    );
}

fn test_mutations(
    initial_revision: u64,
) -> (tempfile::TempDir, Store, TaskMutations, TaskUpdateReceiver) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let (notifier, notifications) = TaskUpdateNotifier::channel();
    let mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(initial_revision))),
        notifier,
    );
    (dir, store, mutations, notifications)
}

fn task_record(task_id: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: "Task".to_string(),
        agent_title: None,
        status: TaskStatus::Inactive,
        task_version: 0,
        message_history_version: 0,
        unread: true,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
        last_activity: "1".to_string(),
        agent_name: "Codex".to_string(),
        agent_id: "codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/tmp/workspace".to_string(),
        first_prompt_sent: true,
        agent_session_id: None,
        active_turn_id: None,
        archived: false,
        tombstoned: false,
        revision: 0,
        config_options: Default::default(),
        config_options_catalog: None,
        agent_commands_catalog: None,
        model_id: None,
        preparation: TaskPreparationRecord::Ready,
    }
}

fn send_receipt(idempotency_key: &str, user_message_id: &str) -> TaskSendReceipt {
    TaskSendReceipt {
        idempotency_key: idempotency_key.to_string(),
        text: "hello".to_string(),
        attachment_handles: Vec::new(),
        user_message_id: user_message_id.to_string(),
        turn_id: "turn-1".to_string(),
    }
}

fn assert_task_unchanged(actual: &TaskRecord, expected: &TaskRecord) {
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

fn assert_rejected_no_change(outcome: TaskCommitOutcome) {
    assert!(matches!(
        outcome,
        TaskCommitOutcome::Rejected(TaskCommitRejection::NoChange)
    ));
}

fn rust_source_files(root: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_rust_source_files(std::path::Path::new(root), &mut files);
    files.sort();
    files
}

fn collect_rust_source_files(path: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_source_files(&path, files);
        } else if is_production_rust_source(&path) {
            files.push(path);
        }
    }
}

fn is_production_rust_source(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    path.extension().and_then(|ext| ext.to_str()) == Some("rs")
        && name != "tests.rs"
        && !name.ends_with("_tests.rs")
}

fn task_updated_calls_by_function(path: &str, text: &str) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    let mut current_function: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(name) = function_name(trimmed) {
            current_function = Some(name.to_string());
        }
        if trimmed.contains(".task_updated(") || trimmed.contains(" task_updated(") {
            calls.push((
                path.to_string(),
                current_function
                    .clone()
                    .unwrap_or_else(|| "<module>".to_string()),
            ));
        }
    }
    calls
}

fn function_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("fn ").or_else(|| {
        line.strip_prefix("pub fn ")
            .or_else(|| line.strip_prefix("pub(crate) fn "))
            .or_else(|| line.strip_prefix("pub(super) fn "))
    })?;
    rest.split('(').next()
}
