//! Task subscription replies by known Chat version (ADR-0086).

use openaide_app_server_protocol::events::TaskChatChange;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::state::TaskChatSync;
use tempfile::TempDir;

use super::*;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, ChatMessage, NormalizedMessage};
use crate::snapshots::TaskSubscription;
use crate::storage::records::TaskRecord;
use crate::storage::task_journal::{TaskOperation, TaskWrite};
use crate::storage::Store;

const TASK: &str = "task-sub";

fn client() -> ClientInstanceId {
    ClientInstanceId::from("client-1")
}

fn open_store(root: &TempDir) -> Store {
    Store::open(root.path().to_path_buf()).unwrap()
}

fn user_message(id: &str) -> ChatMessage {
    let message = NormalizedMessage::User {
        id: id.to_string(),
        text: format!("text of {id}"),
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        attachments: Vec::new(),
    };
    ChatMessage {
        cursor: format!("cursor-{id}"),
        identity: message.identity(),
        message_type: message.message_type().to_string(),
        message_id: message.identity(),
        message,
        timing: Default::default(),
    }
}

/// Commits one message as the normalized operations a live transaction produces, so each is
/// one Chat frame that resumes as a delta.
fn append_message(store: &Store, message: ChatMessage) {
    let mut projection = store.task_journal().load(TASK).unwrap();
    projection.message_meta.version += 1;
    projection.message_meta.message_count += 1;
    let stored = crate::storage::records::StoredMessage {
        sequence: projection.message_meta.message_count,
        chat: message,
    };
    store
        .task_journal()
        .submit(TaskWrite::barrier_operations_with_artifacts(
            TASK.to_string(),
            vec![
                TaskOperation::AppendMessage {
                    message: Box::new(stored),
                },
                TaskOperation::ReplaceMessageMeta {
                    message_meta: Box::new(projection.message_meta),
                },
            ],
            Vec::new(),
            Vec::new(),
        ))
        .unwrap()
        .wait()
        .unwrap();
}

fn append(store: &Store, id: &str) {
    append_message(store, user_message(id));
}

fn version(store: &Store) -> u64 {
    store.task_journal().chat_version(TASK).unwrap()
}

fn task() -> TaskRecord {
    let mut record = super::tests::task_record(TASK);
    record.lifecycle = crate::storage::records::TaskLifecycle::Open;
    record
}

fn subscribe(store: &Store, known: Option<u64>) -> TaskSubscription {
    TaskSnapshotStore::new(store.clone())
        .subscription(&client(), &TaskId::from(TASK), known)
        .unwrap()
}

fn compact(store: &Store) {
    store
        .task_journal()
        .submit(TaskWrite::compaction_barrier(TASK))
        .unwrap()
        .wait()
        .unwrap();
}

fn replace_with_first_message(store: &Store) {
    let mut messages = store.read_messages(TASK).unwrap();
    messages.truncate(1);
    let mut meta = store.task_journal().load(TASK).unwrap().message_meta;
    meta.message_count = 1;
    meta.version += 1;
    store
        .task_journal()
        .submit(TaskWrite::barrier_operations_with_artifacts(
            TASK.to_string(),
            vec![TaskOperation::ReplaceMessages {
                messages,
                message_meta: Box::new(meta),
            }],
            Vec::new(),
            Vec::new(),
        ))
        .unwrap()
        .wait()
        .unwrap();
}

fn store_with_two_messages(root: &TempDir) -> (Store, u64) {
    let store = open_store(root);
    store.write_task(&task()).unwrap();
    let base = version(&store);
    append(&store, "one");
    append(&store, "two");
    (store, base)
}

#[test]
fn the_current_version_replies_with_fields_and_no_chat() {
    let root = TempDir::new().unwrap();
    let (store, _) = store_with_two_messages(&root);
    let head = version(&store);

    let reply = subscribe(&store, Some(head));

    assert!(matches!(reply.chat_sync, TaskChatSync::Current));
    assert_eq!(reply.chat_version, head);
    assert!(reply.task.chat.items.is_empty());
    assert_eq!(reply.task.task.task_id.as_str(), TASK);
}

#[test]
fn an_older_version_replies_with_the_missed_changes_in_order() {
    let root = TempDir::new().unwrap();
    let (store, base) = store_with_two_messages(&root);

    let reply = subscribe(&store, Some(base));

    assert_eq!(reply.chat_version, base + 2);
    let TaskChatSync::Deltas { changes } = reply.chat_sync else {
        panic!("expected deltas");
    };
    let ids = changes
        .iter()
        .map(|change| match change {
            TaskChatChange::Append { item } => item.message_id.as_str().to_string(),
            other => panic!("expected append, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, ["one", "two"]);
}

#[test]
fn no_known_version_or_one_ahead_of_the_task_replies_with_the_tail_page() {
    let root = TempDir::new().unwrap();
    let (store, base) = store_with_two_messages(&root);

    for known in [None, Some(base + 50)] {
        let reply = subscribe(&store, known);
        assert!(matches!(reply.chat_sync, TaskChatSync::Full), "{known:?}");
        assert_eq!(reply.chat_version, base + 2);
        assert_eq!(reply.task.chat.items.len(), 2);
    }
}

#[test]
fn a_version_folded_into_the_snapshot_replies_with_the_tail_page() {
    let root = TempDir::new().unwrap();
    let (store, base) = store_with_two_messages(&root);
    compact(&store);

    let reply = subscribe(&store, Some(base));

    assert!(matches!(reply.chat_sync, TaskChatSync::Full));
    assert_eq!(reply.task.chat.items.len(), 2);
}

#[test]
fn a_version_from_before_a_replace_replies_with_the_tail_page() {
    let root = TempDir::new().unwrap();
    let (store, base) = store_with_two_messages(&root);
    replace_with_first_message(&store);

    let reply = subscribe(&store, Some(base + 2));

    assert!(matches!(reply.chat_sync, TaskChatSync::Full));
    assert_eq!(reply.chat_version, base + 3);
    assert_eq!(reply.task.chat.items.len(), 1);
}

#[test]
fn a_version_cached_before_a_restart_still_resumes() {
    let root = TempDir::new().unwrap();
    let (store, base) = store_with_two_messages(&root);
    let cached = base + 1;
    drop(store);

    let store = open_store(&root);
    let reply = subscribe(&store, Some(cached));

    assert_eq!(reply.chat_version, base + 2);
    let TaskChatSync::Deltas { changes } = reply.chat_sync else {
        panic!("expected deltas after a restart");
    };
    assert_eq!(changes.len(), 1);
    assert!(matches!(
        &changes[0],
        TaskChatChange::Append { item } if item.message_id.as_str() == "two"
    ));
}

#[test]
fn another_clients_prepared_task_is_not_found_with_a_known_version() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    let mut record = task();
    record.lifecycle = crate::storage::records::TaskLifecycle::Prepared {
        lease: Some(ClientInstanceId::from("client-2")),
    };
    store.write_task(&record).unwrap();

    let error = TaskSnapshotStore::new(store)
        .subscription(&client(), &TaskId::from(TASK), Some(1))
        .unwrap_err();

    assert_eq!(
        error.code,
        openaide_app_server_protocol::errors::ProtocolErrorCode::NotFound
    );
}

#[test]
fn agent_text_appends_resolve_to_the_message_they_extend() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root);
    store.write_task(&task()).unwrap();
    let agent = NormalizedMessage::AgentMessage {
        id: "agent-1".to_string(),
        role: AgentMessageRole::Agent,
        parts: vec![AgentMessagePart::Text {
            text: "a".to_string(),
        }],
        created_at: "2026-01-01T00:00:01.000Z".to_string(),
    };
    let mut message = user_message("x");
    message.identity = agent.identity();
    message.message_id = agent.identity();
    message.message_type = agent.message_type().to_string();
    message.message = agent;
    let message_identity = message.identity.clone();
    append_message(&store, message);
    let known = version(&store);
    store
        .task_journal()
        .submit(TaskWrite::barrier_operations_with_artifacts(
            TASK.to_string(),
            vec![TaskOperation::AppendText {
                identity: message_identity,
                text: "b".to_string(),
                local_history_updated_at: "2026-01-01T00:00:02.000Z".to_string(),
                task_updated_at: None,
            }],
            Vec::new(),
            Vec::new(),
        ))
        .unwrap()
        .wait()
        .unwrap();

    let reply = subscribe(&store, Some(known));

    let TaskChatSync::Deltas { changes } = reply.chat_sync else {
        panic!("expected deltas");
    };
    assert!(matches!(
        changes.as_slice(),
        [TaskChatChange::AppendText { message_id, text }]
            if message_id.as_str() == "agent-1" && text == "b"
    ));
}
