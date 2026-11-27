//! Conversion of the generation layout (ADR-0029 as released) to `chat.log`.
//!
//! The fixture under `fixtures/generation-layout-v1` was written in the
//! released format: a `task.json` of schema 1 naming a snapshot and a journal
//! generation, a title, a queued message, and two journal frames (a follow-up
//! message that consumes the queue, then streamed Agent text) past a snapshot
//! holding one Agent message.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tempfile::TempDir;

use crate::protocol::model::{AgentMessagePart, NormalizedMessage};
use crate::storage::records::{QueuedMessageRecord, StoredMessage, TaskTitle};
use crate::storage::task_journal::store::TaskJournalStore;
use crate::storage::task_journal::TaskWrite;

const FIXTURE_TASK: &str = "task_generation_fixture";

const SNAPSHOT: &str = "chat.snapshot.7f3a9c";
const JOURNAL: &str = "chat.journal.7f3a9c";

/// Copies the checked-in fixture into a state root and returns its Task directory.
fn install_fixture(root: &Path) -> PathBuf {
    let task_dir = root.join("task-store-v1/tasks").join(FIXTURE_TASK);
    fs::create_dir_all(&task_dir).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/storage/task_journal/fixtures/generation-layout-v1");
    for entry in fs::read_dir(fixture).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), task_dir.join(entry.file_name())).unwrap();
    }
    task_dir
}

fn task_json(task_dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(task_dir.join("task.json")).unwrap()).unwrap()
}

fn text_of(message: &StoredMessage) -> String {
    match &message.chat.message {
        NormalizedMessage::AgentMessage { parts, .. } => parts
            .iter()
            .map(|part| match part {
                AgentMessagePart::Text { text } => text.as_str(),
                _ => "",
            })
            .collect(),
        NormalizedMessage::User { text, .. } => text.clone(),
        _ => panic!("unexpected message"),
    }
}

fn generation_files(task_dir: &Path) -> Vec<String> {
    fs::read_dir(task_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| super::is_generation_file(name))
        .collect()
}

#[test]
fn a_released_generation_task_converts_lazily_and_keeps_its_task_and_chat_facts() {
    let root = TempDir::new().unwrap();
    let task_dir = install_fixture(root.path());
    let released_task = fs::read(task_dir.join("task.json")).unwrap();

    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    // Startup reads the metadata only: nothing converts before the Chat opens.
    let listed = &store.list_task_records()[0];
    assert_eq!(
        listed.title.as_ref().map(TaskTitle::value),
        Some("Generation fixture")
    );
    assert_eq!(fs::read(task_dir.join("task.json")).unwrap(), released_task);
    assert!(!task_dir.join("chat.log").exists());

    let loaded = store.load(FIXTURE_TASK).unwrap();
    // Chat facts: the snapshot's message with the journal's streamed text, then the follow-up.
    assert_eq!(
        loaded.messages.iter().map(text_of).collect::<Vec<_>>(),
        ["hello world", "follow up"]
    );
    assert_eq!(loaded.message_meta.message_count, 2);
    assert_eq!(
        loaded.task.message_history_version,
        loaded.message_meta.version
    );
    // Task facts: the title and the queue consumption the follow-up committed.
    assert_eq!(
        loaded.task.title.as_ref().map(TaskTitle::value),
        Some("Generation fixture")
    );
    assert!(loaded.task.message_queue.items.is_empty());
    assert_eq!(loaded.task.message_queue.revision, 2);
    assert_eq!(loaded.task.agent_session_id.as_deref(), Some("session_1"));

    // The converted Task is published as a schema 2 `task.json` and one
    // `chat.log` that continues the generation's numbering; the old files are gone.
    let converted = task_json(&task_dir);
    assert_eq!(converted["schemaVersion"], 2);
    assert_eq!(converted["chatFrame"], 2);
    assert!(converted.get("chatSnapshot").is_none() && converted.get("chatJournal").is_none());
    assert!(task_dir.join("chat.log").is_file());
    assert!(generation_files(&task_dir).is_empty());
    assert_eq!(store.chat_version(FIXTURE_TASK).unwrap(), 2);

    // The current writer emits only the new format, and the Task reopens from it.
    store
        .submit(TaskWrite::stream_append_text(
            FIXTURE_TASK,
            "agent_message",
            "!",
            "2026-07-20T00:03:00Z",
        ))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();
    assert!(generation_files(&task_dir).is_empty());
    assert_eq!(task_json(&task_dir)["chatFrame"], 3);
    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(
        text_of(&reopened.load(FIXTURE_TASK).unwrap().messages[0]),
        "hello world!"
    );
    reopened.shutdown().unwrap();
}

#[test]
fn a_failed_conversion_leaves_the_released_bytes_authoritative_and_retryable() {
    let root = TempDir::new().unwrap();
    let task_dir = install_fixture(root.path());
    let before: Vec<_> = ["task.json", SNAPSHOT, JOURNAL]
        .iter()
        .map(|name| fs::read(task_dir.join(name)).unwrap())
        .collect();
    // A directory where the converted log is written makes the conversion fail.
    fs::create_dir(task_dir.join("chat.log.compact")).unwrap();

    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert!(store.load(FIXTURE_TASK).is_err());
    let after: Vec<_> = ["task.json", SNAPSHOT, JOURNAL]
        .iter()
        .map(|name| fs::read(task_dir.join(name)).unwrap())
        .collect();
    assert_eq!(before, after, "the released bytes are untouched");
    assert!(!task_dir.join("chat.log").exists());

    fs::remove_dir(task_dir.join("chat.log.compact")).unwrap();
    assert_eq!(store.load(FIXTURE_TASK).unwrap().messages.len(), 2);
    assert_eq!(task_json(&task_dir)["schemaVersion"], 2);
    store.shutdown().unwrap();
}

#[test]
fn a_journal_frame_past_the_released_checkpoint_rolls_forward_at_startup() {
    let root = TempDir::new().unwrap();
    let task_dir = install_fixture(root.path());
    // The crash left both frames after the checkpoint: task.json is the Task as
    // the snapshot's generation began, with its queue still holding the prompt.
    let mut released = task_json(&task_dir);
    released["chatSequence"] = json!(0);
    released["chatJournalBytes"] = json!(10);
    released["task"]["message_queue"] = json!({
        "revision": 1,
        "items": [serde_json::to_value(QueuedMessageRecord {
            not_before: None,
            queued_message_id: "queue_1".to_string(),
            text: "queued prompt".to_string(),
            created_at: "2026-07-20T00:00:00Z".to_string(),
            chat_attachments: Vec::new(),
            agent_attachments: Vec::new(),
        }).unwrap()],
    });
    fs::write(
        task_dir.join("task.json"),
        serde_json::to_vec(&released).unwrap(),
    )
    .unwrap();

    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    // Startup recovers the accepted follow-up before anything can offer the prompt again.
    let listed = &store.list_task_records()[0];
    assert!(listed.message_queue.items.is_empty());
    assert_eq!(listed.message_queue.revision, 2);
    assert_eq!(task_json(&task_dir)["chatFrame"], 2);
    assert_eq!(store.load(FIXTURE_TASK).unwrap().messages.len(), 2);
    store.shutdown().unwrap();
}

#[test]
fn a_newer_released_chat_in_the_generation_layout_is_set_aside_and_continues_numbering() {
    let root = TempDir::new().unwrap();
    let task_dir = install_fixture(root.path());
    let mut snapshot: Value =
        serde_json::from_slice(&fs::read(task_dir.join(SNAPSHOT)).unwrap()).unwrap();
    snapshot["schemaVersion"] = json!(99);
    fs::write(
        task_dir.join(SNAPSHOT),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();

    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = store.load(FIXTURE_TASK).unwrap();
    assert!(loaded.messages.is_empty());
    assert!(task_dir.join("set-aside").join(SNAPSHOT).is_file());
    assert!(task_dir.join("set-aside").join(JOURNAL).is_file());
    assert!(generation_files(&task_dir).is_empty());
    assert_eq!(task_json(&task_dir)["schemaVersion"], 2);
    // The released generation ended at frame 2, so the rebuilt Chat covers frame 3.
    assert_eq!(store.chat_version(FIXTURE_TASK).unwrap(), 3);
    store.shutdown().unwrap();
}
