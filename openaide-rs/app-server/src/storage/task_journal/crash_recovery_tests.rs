use super::*;
use crate::protocol::model::{ChatMessage, NormalizedMessage};
use crate::storage::records::{QueuedMessageRecord, StoredMessage};

#[test]
fn accepted_first_message_recovers_promotion_before_navigation_or_pool_reconciliation() {
    let root = TempDir::new().unwrap();
    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut initial = task_projection("task_first_send");
    initial.task.lifecycle = TaskLifecycle::Prepared { lease: None };
    store
        .submit(TaskWrite::barrier_create(initial))
        .unwrap()
        .wait()
        .unwrap();
    let metadata_path = root
        .path()
        .join("task-store-v1/tasks/task_first_send/task.json");
    let before_acceptance = std::fs::read(&metadata_path).unwrap();
    let mut accepted = store.load("task_first_send").unwrap();
    accepted.task.lifecycle = TaskLifecycle::Open;
    accept_user_message(&store, &mut accepted, "first prompt");
    store.shutdown().unwrap();

    // Process death after the Chat frame sync but before the atomic metadata
    // replacement leaves exactly these previously published metadata bytes.
    std::fs::write(&metadata_path, before_acceptance).unwrap();
    for _ in 0..2 {
        let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
        let listed = reopened.list_task_records();
        assert_eq!(listed[0].lifecycle, TaskLifecycle::Open);
        assert_eq!(listed[0].revision, 2);
        let loaded = reopened.load("task_first_send").unwrap();
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.task.message_history_version, 1);
        reopened.shutdown().unwrap();
    }
}

#[test]
fn recovered_queue_delivery_never_reoffers_accepted_work_or_reverts_later_metadata() {
    let root = TempDir::new().unwrap();
    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut projection = task_projection("task_queue_delivery");
    store
        .submit(TaskWrite::barrier_create(projection.clone()))
        .unwrap()
        .wait()
        .unwrap();
    accept_user_message(&store, &mut projection, "initial prompt");
    projection
        .task
        .message_queue
        .items
        .push(QueuedMessageRecord {
            queued_message_id: "queue_1".to_string(),
            text: "queued prompt".to_string(),
            created_at: "2026-09-05T00:00:00Z".to_string(),
            chat_attachments: Vec::new(),
            agent_attachments: Vec::new(),
        });
    projection.task.message_queue.revision = 1;
    store
        .submit(TaskWrite::barrier_replace_task(projection.task.clone()))
        .unwrap()
        .wait()
        .unwrap();
    let task_dir = root.path().join("task-store-v1/tasks/task_queue_delivery");
    let before_delivery = std::fs::read(task_dir.join("task.json")).unwrap();
    projection.task.message_queue.items.clear();
    projection.task.message_queue.revision = 2;
    accept_user_message(&store, &mut projection, "queued prompt");
    accept_user_message(&store, &mut projection, "later prompt");
    store.shutdown().unwrap();
    std::fs::write(task_dir.join("task.json"), before_delivery).unwrap();

    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let task = &reopened.list_task_records()[0];
    assert!(task.message_queue.items.is_empty());
    assert_eq!(task.message_queue.revision, 2);
    assert_eq!(task.revision, 4);
    let mut loaded = reopened.load("task_queue_delivery").unwrap();
    assert_eq!(loaded.messages.len(), 3);
    loaded.task.lifecycle = TaskLifecycle::Archived;
    reopened
        .submit(TaskWrite::barrier_replace_task(loaded.task))
        .unwrap()
        .wait()
        .unwrap();
    reopened.shutdown().unwrap();

    // Absence of the additive checkpoint is a valid previous-format metadata
    // file; old Chat frames must not undo the independently committed Archive.
    let mut previous_format: serde_json::Value =
        serde_json::from_slice(&std::fs::read(task_dir.join("task.json")).unwrap()).unwrap();
    previous_format
        .as_object_mut()
        .unwrap()
        .remove("chatJournalBytes");
    std::fs::write(
        task_dir.join("task.json"),
        serde_json::to_vec(&previous_format).unwrap(),
    )
    .unwrap();
    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = reopened.load("task_queue_delivery").unwrap();
    assert_eq!(loaded.task.lifecycle, TaskLifecycle::Archived);
    assert!(loaded.task.message_queue.items.is_empty());
    assert_eq!(loaded.messages.len(), 3);
    reopened.shutdown().unwrap();
}

#[test]
fn incomplete_chat_commit_does_not_promote_or_accept_another_message() {
    let root = TempDir::new().unwrap();
    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut projection = task_projection("task_torn_send");
    store
        .submit(TaskWrite::barrier_create(projection.clone()))
        .unwrap()
        .wait()
        .unwrap();
    accept_user_message(&store, &mut projection, "accepted prompt");
    let task_dir = root.path().join("task-store-v1/tasks/task_torn_send");
    let before_send = std::fs::read(task_dir.join("task.json")).unwrap();
    accept_user_message(&store, &mut projection, "incomplete prompt");
    store.shutdown().unwrap();
    std::fs::write(task_dir.join("task.json"), before_send).unwrap();
    let journal = std::fs::OpenOptions::new()
        .write(true)
        .open(task_dir.join("chat.journal"))
        .unwrap();
    journal
        .set_len(journal.metadata().unwrap().len() - 2)
        .unwrap();
    drop(journal);

    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = reopened.load("task_torn_send").unwrap();
    assert_eq!(loaded.task.revision, 2);
    assert_eq!(loaded.messages.len(), 1);
    reopened.shutdown().unwrap();
}

#[test]
fn interrupted_first_chat_header_leaves_prepared_task_reusable() {
    for header_bytes in [0, 5, 10, 20] {
        let root = TempDir::new().unwrap();
        let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
        let mut projection = task_projection("task_partial_header");
        projection.task.lifecycle = TaskLifecycle::Prepared { lease: None };
        store
            .submit(TaskWrite::barrier_create(projection.clone()))
            .unwrap()
            .wait()
            .unwrap();
        let task_dir = root.path().join("task-store-v1/tasks/task_partial_header");
        let before_send = std::fs::read(task_dir.join("task.json")).unwrap();
        projection.task.lifecycle = TaskLifecycle::Open;
        accept_user_message(&store, &mut projection, "interrupted prompt");
        store.shutdown().unwrap();
        std::fs::write(task_dir.join("task.json"), before_send).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(task_dir.join("chat.journal"))
            .unwrap()
            .set_len(header_bytes)
            .unwrap();

        let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
        let mut loaded = reopened.load("task_partial_header").unwrap();
        assert_eq!(
            loaded.task.lifecycle,
            TaskLifecycle::Prepared { lease: None }
        );
        assert!(loaded.messages.is_empty());
        loaded.task.lifecycle = TaskLifecycle::Open;
        accept_user_message(&reopened, &mut loaded, "accepted prompt");
        assert_eq!(
            reopened.load("task_partial_header").unwrap().messages.len(),
            1
        );
        reopened.shutdown().unwrap();
    }
}

#[test]
fn first_send_of_previous_format_task_establishes_recovery_before_chat_commit() {
    let root = TempDir::new().unwrap();
    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut projection = task_projection("task_upgrade_send");
    projection.task.lifecycle = TaskLifecycle::Prepared { lease: None };
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();
    let metadata_path = root
        .path()
        .join("task-store-v1/tasks/task_upgrade_send/task.json");
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("chatJournalBytes");
    std::fs::write(&metadata_path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut projection = store.load("task_upgrade_send").unwrap();
    // Capture the durable pre-Send state after crossing the normal old-Task
    // hydration/migration seam, before accepting any new Chat transaction.
    let before_send = std::fs::read(&metadata_path).unwrap();
    projection.task.lifecycle = TaskLifecycle::Open;
    accept_user_message(&store, &mut projection, "first prompt after upgrade");
    store.shutdown().unwrap();
    std::fs::write(&metadata_path, before_send).unwrap();

    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(
        reopened.list_task_records()[0].lifecycle,
        TaskLifecycle::Open
    );
    assert_eq!(
        reopened.load("task_upgrade_send").unwrap().messages.len(),
        1
    );
    reopened.shutdown().unwrap();
}

#[test]
fn streamed_chat_does_not_duplicate_unchanged_queued_content_in_its_recovery_record() {
    let root = TempDir::new().unwrap();
    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut projection = task_projection("task_bounded_recovery");
    projection
        .task
        .message_queue
        .items
        .push(QueuedMessageRecord {
            queued_message_id: "queue_large".to_string(),
            text: "q".repeat(1024 * 1024),
            created_at: "2026-09-05T00:00:00Z".to_string(),
            chat_attachments: Vec::new(),
            agent_attachments: Vec::new(),
        });
    let stored = StoredMessage {
        sequence: 1,
        chat: ChatMessage {
            cursor: "1".to_string(),
            message_id: "agent_1".to_string(),
            identity: "agent_1".to_string(),
            message_type: "agent_message".to_string(),
            message: NormalizedMessage::AgentMessage {
                id: "agent_1".to_string(),
                role: crate::protocol::model::AgentMessageRole::Agent,
                parts: vec![crate::protocol::model::AgentMessagePart::Text {
                    text: String::new(),
                }],
                created_at: "2026-09-05T00:00:00Z".to_string(),
            },
        },
    };
    projection.messages.push(stored.clone());
    projection.message_meta.message_count = 1;
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    for _ in 0..4 {
        store
            .submit(TaskWrite::stream_append_text(
                "task_bounded_recovery",
                "agent_1",
                "x",
                "1",
            ))
            .unwrap()
            .wait()
            .unwrap();
    }
    store.shutdown().unwrap();
    let task_dir = root
        .path()
        .join("task-store-v1/tasks/task_bounded_recovery");
    assert!(
        std::fs::metadata(task_dir.join("chat.journal"))
            .unwrap()
            .len()
            < 64 * 1024,
        "tiny Chat updates must not duplicate unrelated megabytes of queued work"
    );
    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = reopened.load("task_bounded_recovery").unwrap();
    assert_eq!(loaded.task.message_queue.items[0].text.len(), 1024 * 1024);
    assert!(matches!(
        &loaded.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if parts == &[crate::protocol::model::AgentMessagePart::Text { text: "xxxx".to_string() }]
    ));
    reopened.shutdown().unwrap();
}

fn accept_user_message(store: &TaskJournalStore, projection: &mut TaskProjection, text: &str) {
    projection.task.revision += 1;
    projection.task.task_version += 1;
    projection.task.message_history_version += 1;
    projection.message_meta.version += 1;
    projection.message_meta.message_count += 1;
    let message = StoredMessage {
        sequence: projection.message_meta.message_count,
        chat: ChatMessage {
            cursor: projection.message_meta.message_count.to_string(),
            message_id: text.to_string(),
            identity: text.to_string(),
            message_type: "user".to_string(),
            message: NormalizedMessage::User {
                id: text.to_string(),
                text: text.to_string(),
                created_at: "2026-09-05T00:00:00Z".to_string(),
                attachments: Vec::new(),
            },
        },
    };
    projection.messages.push(message.clone());
    // These are the normalized operations emitted by TaskMutations when Send
    // accepts a User message together with its lifecycle and queue changes.
    store
        .submit(TaskWrite::barrier_operations_with_artifacts(
            projection.task.task_id.clone(),
            vec![
                TaskOperation::ReplaceTask {
                    task: Box::new(projection.task.clone()),
                },
                TaskOperation::AppendMessage {
                    message: Box::new(message),
                },
                TaskOperation::ReplaceMessageMeta {
                    message_meta: Box::new(projection.message_meta.clone()),
                },
            ],
            Vec::new(),
            Vec::new(),
        ))
        .unwrap()
        .wait()
        .unwrap();
}
