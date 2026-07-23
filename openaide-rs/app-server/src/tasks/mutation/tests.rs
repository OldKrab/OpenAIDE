use std::sync::{Arc, Mutex};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentMessagePart, AgentMessageRole, IsolationKind, NormalizedMessage, TaskStatus,
};
use crate::storage::records::{TaskLifecycle, TaskPreparationRecord, TaskRecord};
use crate::storage::Store;
use crate::task_events::{
    CommittedNavigationChange, TaskUpdateKind, TaskUpdateNotifier, TaskUpdateReceiver,
};
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
fn navigation_event_kind_follows_row_vs_collection_ownership() {
    let (_dir, store, mutations, notifications) = test_mutations(0);
    store.write_task(&task_record("task_navigation")).unwrap();

    mutations
        .commit_existing_task("task_navigation", TaskCommitOptions::metadata(), |ctx| {
            ctx.task_mut().status = TaskStatus::Waiting;
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();
    let row_update = notifications.try_recv().unwrap();
    assert!(matches!(
        row_update.kind,
        TaskUpdateKind::Changed(change)
            if matches!(
                change.navigation,
                Some(CommittedNavigationChange::TaskUpdated(_))
            )
    ));

    mutations
        .commit_existing_task("task_navigation", TaskCommitOptions::metadata(), |ctx| {
            ctx.task_mut().lifecycle = TaskLifecycle::Archived;
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();
    let collection_update = notifications.try_recv().unwrap();
    assert!(matches!(
        collection_update.kind,
        TaskUpdateKind::Changed(change)
            if matches!(
                change.navigation,
                Some(CommittedNavigationChange::ProjectEntriesChanged { .. })
            )
    ));
}

#[test]
fn replacing_task_runtime_routes_terminal_commits_to_the_new_notifier() {
    let (_dir, store, _first_mutations, first_notifications) = test_mutations(0);
    store.write_task(&task_record("task_runtime_swap")).unwrap();
    let (second_notifier, second_notifications) = TaskUpdateNotifier::channel();
    let _second_mutations = TaskMutations::new(
        store.clone(),
        Arc::new(Mutex::new(())),
        Arc::new(Mutex::new(RuntimeState::with_revision(0))),
        second_notifier,
    );

    store
        .task_journal()
        .submit(
            crate::storage::task_journal::TaskWrite::stream_append_terminal(
                "task_runtime_swap",
                "artifact_1",
                "terminal_1",
                "durable",
            ),
        )
        .unwrap();
    store
        .task_journal()
        .submit(crate::storage::task_journal::TaskWrite::barrier(
            "task_runtime_swap",
        ))
        .unwrap()
        .wait()
        .unwrap();

    let update = second_notifications
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("replacement runtime receives terminal publication");
    assert!(matches!(
        update.kind,
        crate::task_events::TaskUpdateKind::ToolDetailChanged { .. }
    ));
    assert!(first_notifications.try_recv().is_err());
}

#[test]
fn commit_rejects_clearing_or_replacing_a_bound_native_session() {
    let (_dir, store, mutations, notifications) = test_mutations(0);
    for (task_id, replacement) in [
        ("task_clear_session", None),
        ("task_replace_session", Some("session-2".to_string())),
    ] {
        let mut record = task_record(task_id);
        record.agent_session_id = Some("session-1".to_string());
        store.write_task(&record).unwrap();

        let error = mutations
            .commit_existing_task(task_id, TaskCommitOptions::metadata(), |ctx| {
                ctx.task_mut().agent_session_id = replacement;
                Ok(TaskMutationResult::Changed)
            })
            .unwrap_err();

        assert!(matches!(error, RuntimeError::Internal(message) if
            message == "task mutation changed bound Native Session identity"));
        assert_task_unchanged(&store.read_task(task_id).unwrap(), &record);
    }
    assert!(notifications.try_recv().is_err());
}

#[test]
fn queued_updates_keep_values_from_their_own_committed_revision() {
    let (_dir, store, mutations, notifications) = test_mutations(0);
    store.write_task(&task_record("task_ordered")).unwrap();

    for title in ["First", "Second"] {
        mutations
            .commit_existing_task("task_ordered", TaskCommitOptions::metadata(), |ctx| {
                ctx.task_mut().title = crate::storage::records::TaskTitleState::from_title(
                    crate::storage::records::TaskTitle::new(
                        title,
                        crate::storage::records::TaskTitleSource::User,
                    ),
                );
                Ok(TaskMutationResult::Changed)
            })
            .unwrap();
    }

    let titles = [notifications.recv().unwrap(), notifications.recv().unwrap()].map(|update| {
        match update.kind {
            crate::task_events::TaskUpdateKind::Changed(change) => change
                .changes
                .task
                .and_then(|task| task.title)
                .map(|title| title.value)
                .expect("summary change should contain its committed title"),
            other => panic!("expected Task change, got {other:?}"),
        }
    });

    assert_eq!(titles, ["First", "Second"]);
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
            ctx.task_mut().title = crate::storage::records::TaskTitleState::from_title(
                crate::storage::records::TaskTitle::new(
                    "Should not persist",
                    crate::storage::records::TaskTitleSource::User,
                ),
            );
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
                ctx.task_mut().title = crate::storage::records::TaskTitleState::from_title(
                    crate::storage::records::TaskTitle::new(
                        "Updated",
                        crate::storage::records::TaskTitleSource::User,
                    ),
                );
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
                ctx.append_message(NormalizedMessage::AgentMessage {
                    id: "message_2".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "should roll back".to_string(),
                    }],
                    created_at: "3".to_string(),
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
fn rejected_commit_rolls_back_durable_agent_text_chunk() {
    let (_dir, store, mutations, notifications) = test_mutations(3);
    let mut record = task_record("task_reject_agent_chunk");
    record.revision = 3;
    store.write_task(&record).unwrap();
    mutations
        .append_message(
            "task_reject_agent_chunk",
            NormalizedMessage::AgentMessage {
                id: "agent-message".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: "original".to_string(),
                }],
                created_at: "2".to_string(),
            },
        )
        .unwrap();

    let result = mutations
        .commit_existing_task(
            "task_reject_agent_chunk",
            TaskCommitOptions::metadata(),
            |ctx| {
                ctx.append_agent_message_part(NormalizedMessage::AgentMessage {
                    id: "agent-message".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: " should roll back".to_string(),
                    }],
                    created_at: "3".to_string(),
                })?;
                Ok(TaskMutationResult::Rejected)
            },
        )
        .unwrap();

    assert_rejected_no_change(result.outcome);
    let messages = store.read_messages("task_reject_agent_chunk").unwrap();
    let NormalizedMessage::AgentMessage { parts, .. } = &messages[0].chat.message else {
        panic!("expected Agent message");
    };
    assert_eq!(
        parts,
        &[AgentMessagePart::Text {
            text: "original".to_string(),
        }]
    );
    assert_eq!(
        store.read_task("task_reject_agent_chunk").unwrap().revision,
        3
    );
    assert_eq!(mutations.current_revision(), 3);
    assert!(notifications.try_recv().is_err());
}

#[test]
fn streamed_agent_text_materializes_large_history_only_once() {
    let (dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record("task_stream_cache")).unwrap();
    let mut history = (0..2_000)
        .map(|index| NormalizedMessage::User {
            id: format!("history-{index}"),
            text: "x".repeat(600),
            created_at: "1".to_string(),
            attachments: Vec::new(),
        })
        .collect::<Vec<_>>();
    history.push(NormalizedMessage::AgentMessage {
        id: "agent-message".to_string(),
        role: AgentMessageRole::Agent,
        parts: vec![AgentMessagePart::Text {
            text: "start".to_string(),
        }],
        created_at: "2".to_string(),
    });
    store
        .replace_messages_with_normalized("task_stream_cache", history)
        .unwrap();
    let journal = dir
        .path()
        .join("task-store-v1/tasks/task_stream_cache/chat.journal");
    let before_stream = journal.metadata().unwrap().len();

    for text in [" first", " second"] {
        mutations
            .commit_existing_task(
                "task_stream_cache",
                TaskCommitOptions {
                    refresh_message_history: true,
                    response_snapshot_tail_limit: None,
                },
                |ctx| {
                    ctx.append_agent_message_part(NormalizedMessage::AgentMessage {
                        id: "agent-message".to_string(),
                        role: AgentMessageRole::Agent,
                        parts: vec![AgentMessagePart::Text {
                            text: text.to_string(),
                        }],
                        created_at: "3".to_string(),
                    })?;
                    Ok(TaskMutationResult::Changed)
                },
            )
            .unwrap();
        if text == " first" {
            assert_eq!(store.message_file_read_count_for_test(), 0);
        }
    }
    let streamed_bytes = journal.metadata().unwrap().len() - before_stream;
    assert!(
        streamed_bytes < 32 * 1024,
        "two text deltas rewrote unchanged history ({streamed_bytes} bytes)"
    );

    let reads_after_stream = store.message_file_read_count_for_test();
    mutations
        .commit_existing_task(
            "task_stream_cache",
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                ctx.append_agent_message_part(NormalizedMessage::AgentMessage {
                    id: "agent-message".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: " third".to_string(),
                    }],
                    created_at: "4".to_string(),
                })?;
                Ok(TaskMutationResult::Changed)
            },
        )
        .unwrap();

    assert_eq!(store.message_file_read_count_for_test(), reads_after_stream);
    let messages = store.read_messages("task_stream_cache").unwrap();
    let NormalizedMessage::AgentMessage { parts, .. } = &messages.last().unwrap().chat.message
    else {
        panic!("expected final Agent message");
    };
    assert_eq!(
        parts,
        &[AgentMessagePart::Text {
            text: "start first second third".to_string(),
        }]
    );
}

#[test]
fn one_transaction_persists_every_text_chunk_exactly_once() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    store.write_task(&task_record("task_multi_chunk")).unwrap();
    mutations
        .commit_existing_task("task_multi_chunk", TaskCommitOptions::metadata(), |ctx| {
            ctx.append_message(NormalizedMessage::AgentMessage {
                id: "agent-message".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: "start".to_string(),
                }],
                created_at: "2".to_string(),
            })?;
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();

    mutations
        .commit_existing_task("task_multi_chunk", TaskCommitOptions::metadata(), |ctx| {
            for text in [" one", " two"] {
                ctx.append_agent_message_part(NormalizedMessage::AgentMessage {
                    id: "agent-message".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: text.to_string(),
                    }],
                    created_at: "3".to_string(),
                })?;
            }
            Ok(TaskMutationResult::Changed)
        })
        .unwrap();

    let messages = store.read_messages("task_multi_chunk").unwrap();
    let NormalizedMessage::AgentMessage { parts, .. } = &messages[0].chat.message else {
        panic!("expected Agent message");
    };
    assert_eq!(
        parts,
        &[AgentMessagePart::Text {
            text: "start one two".to_string(),
        }]
    );
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
                ctx.append_message(NormalizedMessage::AgentMessage {
                    id: "message_1".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: "should roll back".to_string(),
                    }],
                    created_at: "3".to_string(),
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
    assert_eq!(facts.revision, 1);
    assert!(result.response_snapshot.is_some());

    let stored = store.read_task("task_create_commit").unwrap();
    assert_eq!(stored.task_version, 1);
    assert_eq!(stored.revision, 1);
    assert_eq!(stored.message_history_version, 1);
    assert_eq!(store.read_messages("task_create_commit").unwrap().len(), 1);
    assert_eq!(mutations.current_revision(), 11);

    let notification = notifications.try_recv().unwrap();
    assert_eq!(notification.task_id, "task_create_commit");
    assert_eq!(notification.revision, 1);
}

#[test]
fn task_revisions_are_consecutive_across_interleaved_task_commits() {
    let (_dir, store, mutations, notifications) = test_mutations(0);
    store.write_task(&task_record("task-a")).unwrap();
    store.write_task(&task_record("task-b")).unwrap();

    for task_id in ["task-a", "task-b", "task-a"] {
        mutations
            .commit_existing_task(task_id, TaskCommitOptions::metadata(), |ctx| {
                let unread = ctx.task().unread;
                ctx.task_mut().unread = !unread;
                Ok(TaskMutationResult::Changed)
            })
            .unwrap();
    }

    assert_eq!(store.read_task("task-a").unwrap().revision, 2);
    assert_eq!(store.read_task("task-b").unwrap().revision, 1);
    let revisions = [
        notifications.recv().unwrap(),
        notifications.recv().unwrap(),
        notifications.recv().unwrap(),
    ]
    .map(|update| (update.task_id, update.revision));
    assert_eq!(
        revisions,
        [
            ("task-a".to_string(), 1),
            ("task-b".to_string(), 1),
            ("task-a".to_string(), 2),
        ]
    );
}

#[test]
fn create_task_persists_initial_history_in_one_message_batch() {
    let (dir, store, mutations, _notifications) = test_mutations(0);
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

    assert_eq!(store.message_file_write_count_for_test(), 0);
    assert_eq!(store.read_messages("task_bulk_history").unwrap().len(), 3);
    assert!(!dir
        .path()
        .join("tasks/task_bulk_history/messages.jsonl")
        .exists());
    let task_dir = dir.path().join("task-store-v1/tasks/task_bulk_history");
    assert!(task_dir.join("task.json").exists());
    assert!(task_dir.join("chat.snapshot").exists());
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
    assert!(store.read_messages("task_create_write_failure").is_err());
    assert!(notifications.try_recv().is_err());
}

#[test]
fn migrated_service_paths_have_no_direct_task_changed_calls() {
    let allowed = [("src/tasks/mutation/commit.rs", "notify_task_changed")];
    let mut actual = Vec::new();

    for path in rust_source_files("src/tasks") {
        let path_string = path
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(&path)
            .to_string_lossy()
            .trim_start_matches('/')
            .replace('\\', "/");
        let text = std::fs::read_to_string(&path).unwrap();
        actual.extend(task_changed_calls_by_function(&path_string, &text));
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
        .chain([
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tasks/turn_lifecycle.rs")
        ]);

    for path in lifecycle_paths {
        let path_string = path.to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&path).unwrap();
        for (line_index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            for pattern in [
                ".task_changed(",
                " task_changed(",
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

#[test]
fn stale_release_from_an_old_client_does_not_unlock_a_newer_lease() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    let current_client = openaide_app_server_protocol::ids::ClientInstanceId::from("client-new");
    let mut task = task_record("task-prepared");
    task.lifecycle = TaskLifecycle::Prepared {
        lease: Some(current_client.clone()),
    };
    store.write_task(&task).unwrap();

    let disposed = mutations
        .release_prepared_task(
            &openaide_app_server_protocol::ids::ClientInstanceId::from("client-old"),
            "task-prepared",
            "2",
        )
        .unwrap();

    assert!(disposed.is_empty());
    assert_eq!(
        store.read_task("task-prepared").unwrap().lifecycle,
        TaskLifecycle::Prepared {
            lease: Some(current_client)
        }
    );
}

#[test]
fn releasing_a_second_task_for_the_same_key_disposes_the_extra() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    for (task_id, client_id) in [("task-a", "client-a"), ("task-b", "client-b")] {
        let mut task = task_record(task_id);
        task.lifecycle = TaskLifecycle::Prepared {
            lease: Some(client_id.into()),
        };
        store.write_task(&task).unwrap();
    }

    assert!(mutations
        .release_prepared_task(&"client-a".into(), "task-a", "2")
        .unwrap()
        .is_empty());
    let disposed = mutations
        .release_prepared_task(&"client-b".into(), "task-b", "3")
        .unwrap();

    assert_eq!(disposed.len(), 1);
    assert_eq!(disposed[0].task_id, "task-b");
    assert!(!store.read_task("task-a").unwrap().tombstoned);
    assert!(store.read_task("task-b").unwrap().tombstoned);
}

#[test]
fn free_pool_evicts_the_oldest_entry_after_the_global_cap() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    for index in 0..=8 {
        let mut task = task_record(&format!("task-{index}"));
        task.workspace_root = format!("/tmp/workspace-{index}");
        task.lifecycle = TaskLifecycle::Prepared {
            lease: Some(format!("client-{index}").into()),
        };
        store.write_task(&task).unwrap();
        mutations
            .release_prepared_task(
                &format!("client-{index}").into(),
                &format!("task-{index}"),
                &format!("{index:02}"),
            )
            .unwrap();
    }

    assert!(store.read_task("task-0").unwrap().tombstoned);
    for index in 1..=8 {
        assert!(
            !store
                .read_task(&format!("task-{index}"))
                .unwrap()
                .tombstoned
        );
    }
}

#[test]
fn disabling_an_agent_disposes_its_leased_and_free_prepared_tasks_only() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    for (task_id, lifecycle, agent_id) in [
        (
            "leased",
            TaskLifecycle::Prepared {
                lease: Some("client-a".into()),
            },
            "codex",
        ),
        ("free", TaskLifecycle::Prepared { lease: None }, "codex"),
        ("visible", TaskLifecycle::Open, "codex"),
        (
            "other-agent",
            TaskLifecycle::Prepared { lease: None },
            "opencode",
        ),
    ] {
        let mut task = task_record(task_id);
        task.lifecycle = lifecycle;
        task.agent_id = agent_id.to_string();
        store.write_task(&task).unwrap();
    }

    let disposed = mutations.dispose_prepared_tasks_for_agent("codex").unwrap();

    assert_eq!(disposed.len(), 2);
    assert!(store.read_task("leased").unwrap().tombstoned);
    assert!(store.read_task("free").unwrap().tombstoned);
    assert!(!store.read_task("visible").unwrap().tombstoned);
    assert!(!store.read_task("other-agent").unwrap().tombstoned);
}

#[test]
fn changed_agent_preferences_dispose_only_free_prepared_tasks() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    for (task_id, lifecycle, agent_id) in [
        (
            "leased",
            TaskLifecycle::Prepared {
                lease: Some("client-a".into()),
            },
            "codex",
        ),
        ("free", TaskLifecycle::Prepared { lease: None }, "codex"),
        (
            "other-agent",
            TaskLifecycle::Prepared { lease: None },
            "opencode",
        ),
    ] {
        let mut task = task_record(task_id);
        task.lifecycle = lifecycle;
        task.agent_id = agent_id.to_string();
        store.write_task(&task).unwrap();
    }

    let disposed = mutations
        .dispose_free_prepared_tasks_for_agent("codex")
        .unwrap();

    assert_eq!(disposed.len(), 1);
    assert!(!store.read_task("leased").unwrap().tombstoned);
    assert!(store.read_task("free").unwrap().tombstoned);
    assert!(!store.read_task("other-agent").unwrap().tombstoned);
}

#[test]
fn removing_a_worktree_disposes_its_leased_and_free_prepared_tasks_only() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    for (task_id, lifecycle, worktree_id) in [
        (
            "leased",
            TaskLifecycle::Prepared {
                lease: Some("client-a".into()),
            },
            Some("worktree-a"),
        ),
        (
            "free",
            TaskLifecycle::Prepared { lease: None },
            Some("worktree-a"),
        ),
        ("visible", TaskLifecycle::Open, Some("worktree-a")),
        (
            "other-worktree",
            TaskLifecycle::Prepared { lease: None },
            Some("worktree-b"),
        ),
        (
            "project-root",
            TaskLifecycle::Prepared { lease: None },
            None,
        ),
    ] {
        let mut task = task_record(task_id);
        task.lifecycle = lifecycle;
        task.worktree_id = worktree_id.map(str::to_string);
        store.write_task(&task).unwrap();
    }

    let disposed = mutations
        .dispose_prepared_tasks_for_worktree("worktree-a")
        .unwrap();

    assert_eq!(disposed.len(), 2);
    assert!(store.read_task("leased").unwrap().tombstoned);
    assert!(store.read_task("free").unwrap().tombstoned);
    assert!(!store.read_task("visible").unwrap().tombstoned);
    assert!(!store.read_task("other-worktree").unwrap().tombstoned);
    assert!(!store.read_task("project-root").unwrap().tombstoned);
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

#[test]
fn terminal_stream_backpressure_is_waited_outside_the_global_mutation_lock() {
    let source = include_str!("../mutation.rs");
    let function = source
        .split("pub(crate) fn append_terminal_outputs")
        .nth(1)
        .unwrap()
        .split("#[cfg(test)]")
        .next()
        .unwrap();
    let release = function.find("drop(guard)").unwrap();
    let wait = function.find("wait_for_capacity").unwrap();
    assert!(
        release < wait,
        "backpressure must not retain the workflow lock"
    );
}

#[test]
fn terminal_stream_revalidates_session_after_waiting_for_the_mutation_lock() {
    let (_dir, store, mutations, _notifications) = test_mutations(0);
    let mut task = task_record("task_terminal_admission");
    task.agent_session_id = Some("session_1".to_string());
    store.write_task(&task).unwrap();

    let guard = mutations.lock();
    let worker_mutations = mutations.clone();
    let (result, finished) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let outcome = worker_mutations.append_terminal_outputs(
            "task_terminal_admission",
            "session_1",
            vec![crate::storage::task_journal::ToolTerminalAppend {
                artifact_id: "artifact_1".to_string(),
                terminal_id: "terminal_1".to_string(),
                data: "output".to_string(),
            }],
        );
        result.send(outcome).unwrap();
    });

    assert!(finished
        .recv_timeout(std::time::Duration::from_millis(100))
        .is_err());
    drop(guard);
    let outcome = finished.recv_timeout(std::time::Duration::from_secs(1));
    worker.join().unwrap();
    assert!(outcome
        .expect("terminal admission must resume after the workflow mutation")
        .is_ok());

    let mut replacement = store.read_task("task_terminal_admission").unwrap();
    replacement.agent_session_id = Some("session_2".to_string());
    store.write_task(&replacement).unwrap();
    let error = mutations
        .append_terminal_outputs(
            "task_terminal_admission",
            "session_1",
            vec![crate::storage::task_journal::ToolTerminalAppend {
                artifact_id: "artifact_1".to_string(),
                terminal_id: "terminal_1".to_string(),
                data: "stale".to_string(),
            }],
        )
        .expect_err("stale Native Session output must be rejected after lock wait");
    assert!(error.to_string().contains("stale Native Session"));
}

fn task_record(task_id: &str) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitleState::from_title(
            crate::storage::records::TaskTitle::new(
                "Task",
                crate::storage::records::TaskTitleSource::User,
            ),
        ),
        status: TaskStatus::Inactive,
        task_version: 0,
        message_history_version: 0,
        unread: true,
        attention: None,
        created_at: "1".to_string(),
        updated_at: "1".to_string(),
        last_activity: "1".to_string(),
        agent_name: "Codex".to_string(),
        agent_id: "codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/tmp/workspace".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        tombstoned: false,
        revision: 0,
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
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
    collect_rust_source_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(root),
        &mut files,
    );
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

fn task_changed_calls_by_function(path: &str, text: &str) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    let mut current_function: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(name) = function_name(trimmed) {
            current_function = Some(name.to_string());
        }
        if trimmed.contains(".task_changed(") || trimmed.contains(" task_changed(") {
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
