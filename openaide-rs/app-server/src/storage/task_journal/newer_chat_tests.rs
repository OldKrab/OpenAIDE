use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tempfile::TempDir;

use super::super::frame::{self, FramedRecord};
use super::super::split::CHAT_SCHEMA_VERSION;
use super::super::store::TaskJournalStore;
use super::super::TaskWrite;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, ChatMessage, NormalizedMessage};
use crate::storage::records::StoredMessage;

const TASK_ID: &str = "task_newer_chat";

/// A Task written by this App Server, with one message, an image file, a Tool
/// artifact file, and Subagent files, so a set-aside has every part to move.
fn stored_task(root: &Path) -> PathBuf {
    let (store, _commits) = TaskJournalStore::open(root.to_path_buf()).unwrap();
    let mut projection = super::super::store::tests::task_projection(TASK_ID);
    projection.messages.push(StoredMessage {
        sequence: 1,
        chat: ChatMessage {
            cursor: "1".to_string(),
            identity: "message_1".to_string(),
            message_type: "agent_message".to_string(),
            message_id: "message_1".to_string(),
            message: NormalizedMessage::AgentMessage {
                id: "message_1".to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: "set aside".to_string(),
                }],
                created_at: "2026-07-20T00:00:00Z".to_string(),
            },
            timing: Default::default(),
        },
    });
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();
    let task_dir = root.join("task-store-v1/tasks").join(TASK_ID);
    for (dir, file) in [
        ("images", "image_1"),
        ("artifacts", "artifact_1.journal"),
        ("subagents", "catalog.json"),
    ] {
        fs::create_dir_all(task_dir.join(dir)).unwrap();
        fs::write(task_dir.join(dir).join(file), b"{\"schemaVersion\":1}").unwrap();
    }
    task_dir
}

fn metadata(task_dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(task_dir.join("task.json")).unwrap()).unwrap()
}

fn write_metadata(task_dir: &Path, metadata: &Value) {
    fs::write(
        task_dir.join("task.json"),
        serde_json::to_vec(metadata).unwrap(),
    )
    .unwrap();
}

/// Rewrites the snapshot record of the Task's `chat.log` at another schema
/// version, keeping every frame after it.
fn set_snapshot_version(task_dir: &Path, version: u16) -> PathBuf {
    let log = task_dir.join("chat.log");
    let bytes = fs::read(&log).unwrap();
    let header_len = 18;
    let length = u64::from_le_bytes(bytes[header_len..header_len + 8].try_into().unwrap()) as usize;
    let payload = &bytes[header_len + 8..header_len + 8 + length];
    let mut value: Value = serde_json::from_slice(payload).unwrap();
    value["schemaVersion"] = json!(version);
    let payload = serde_json::to_vec(&value).unwrap();
    let mut rewritten = bytes[..header_len].to_vec();
    rewritten.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    rewritten.extend_from_slice(&payload);
    rewritten.extend_from_slice(&frame::crc32(&payload).to_le_bytes());
    rewritten.extend_from_slice(&bytes[header_len + 8 + length + 4..]);
    fs::write(&log, rewritten).unwrap();
    log
}

/// Commits one log frame at `schema_version`, as a newer App Server would.
fn commit_frame(task_dir: &Path, schema_version: u16) {
    #[derive(Serialize, Deserialize)]
    struct FutureFrame {
        format_version: u16,
        schema_version: u16,
        sequence: u64,
        operations: Vec<Value>,
        future_field: String,
    }
    impl FramedRecord for FutureFrame {
        fn format_version(&self) -> u16 {
            self.format_version
        }
        fn sequence(&self) -> u64 {
            self.sequence
        }
    }
    let mut metadata = metadata(task_dir);
    let next = metadata["chatFrame"].as_u64().unwrap() + 1;
    frame::append(
        &task_dir.join("chat.log"),
        &FutureFrame {
            format_version: 1,
            schema_version,
            sequence: next,
            operations: vec![json!({ "futureOperation": {} })],
            future_field: "unknown to this App Server".to_string(),
        },
    )
    .unwrap();
    metadata["chatFrame"] = json!(next);
    metadata["storageSequence"] = json!(metadata["storageSequence"].as_u64().unwrap() + 1);
    write_metadata(task_dir, &metadata);
}

fn open(root: &Path) -> TaskJournalStore {
    TaskJournalStore::open(root.to_path_buf()).unwrap().0
}

/// The Task loads with an empty Chat that the next open fills from its
/// Native Session, and every Chat part of the newer release is set aside.
fn assert_rebuilt(store: &TaskJournalStore, task_dir: &Path) {
    let loaded = store
        .load(TASK_ID)
        .expect("a newer Chat is rebuilt, not refused");
    assert!(loaded.messages.is_empty());
    assert_eq!(loaded.task.agent_session_id.as_deref(), Some("session_1"));
    assert!(loaded.task.native_session_reload_requirement.is_some());
    assert!(loaded.message_meta.version > 0);

    let durable = metadata(task_dir);
    assert!(durable["task"]["native_session_reload_requirement"].is_object());
    assert_eq!(
        log_snapshot_version(&task_dir.join("chat.log")),
        Some(CHAT_SCHEMA_VERSION),
        "the empty Chat is this release's format"
    );
    let set_aside = task_dir.join("set-aside");
    assert!(set_aside.join("chat.log").is_file());
    for (dir, file) in [
        ("images", "image_1"),
        ("artifacts", "artifact_1.journal"),
        ("subagents", "catalog.json"),
    ] {
        assert!(
            set_aside.join(dir).join(file).is_file(),
            "{dir} is set aside"
        );
        assert!(!task_dir.join(dir).exists(), "{dir} leaves the Task");
    }
    assert!(!task_dir.join("set-aside.pending").exists());
}

fn log_snapshot_version(path: &Path) -> Option<u16> {
    super::super::chat_log::recorded_versions(path)
        .unwrap()
        .snapshot_schema
}

#[test]
fn a_snapshot_from_a_newer_release_is_set_aside_and_rebuilt() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    set_snapshot_version(&task_dir, CHAT_SCHEMA_VERSION + 1);

    let store = open(root.path());
    assert_rebuilt(&store, &task_dir);
    assert_eq!(
        log_snapshot_version(&task_dir.join("set-aside/chat.log")),
        Some(CHAT_SCHEMA_VERSION + 1),
        "the newer bytes are kept unchanged"
    );
    store.shutdown().unwrap();
}

#[test]
fn a_journal_frame_from_a_newer_release_is_set_aside_and_rebuilt() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    commit_frame(&task_dir, CHAT_SCHEMA_VERSION + 1);

    let store = open(root.path());
    assert_rebuilt(&store, &task_dir);
    store.shutdown().unwrap();
}

#[test]
fn a_log_file_format_from_a_newer_release_is_set_aside_and_rebuilt() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    commit_frame(&task_dir, CHAT_SCHEMA_VERSION);
    let path = task_dir.join("chat.log");
    let mut bytes = fs::read(&path).unwrap();
    bytes[8..10].copy_from_slice(&(super::super::chat_log::LOG_FORMAT_VERSION + 1).to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let store = open(root.path());
    assert_rebuilt(&store, &task_dir);
    store.shutdown().unwrap();
}

#[test]
fn a_newer_subagent_catalog_sets_aside_a_readable_chat() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    fs::write(
        task_dir.join("subagents/catalog.json"),
        serde_json::to_vec(&json!({ "schemaVersion": 99, "entries": [] })).unwrap(),
    )
    .unwrap();

    let store = open(root.path());
    assert_rebuilt(&store, &task_dir);
    store.shutdown().unwrap();
}

#[test]
fn a_damaged_chat_still_fails_closed() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    fs::write(task_dir.join("chat.log"), b"not a chat log").unwrap();

    let store = open(root.path());
    assert!(store.load(TASK_ID).is_err());
    assert!(!task_dir.join("set-aside").exists());
    assert!(!task_dir.join("set-aside.pending").exists());
    assert!(task_dir.join("images/image_1").is_file(), "nothing moves");
    store.shutdown().unwrap();
}

/// Starts a set-aside as a rebuild does, then moves one part before a crash.
fn interrupt_before_commit(task_dir: &Path, moved: &str) {
    let file = super::super::split::read_task_file(task_dir)
        .unwrap()
        .unwrap();
    let newer = super::detect(task_dir).expect("the Chat is newer");
    let pending = super::begin(task_dir, &super::chat_identity(&file), &newer).unwrap();
    fs::rename(task_dir.join(moved), pending.join(moved)).unwrap();
}

#[test]
fn a_set_aside_interrupted_before_its_commit_resumes_with_what_it_moved() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    set_snapshot_version(&task_dir, CHAT_SCHEMA_VERSION + 1);
    interrupt_before_commit(&task_dir, "images");

    let store = open(root.path());
    assert_rebuilt(&store, &task_dir);
    store.shutdown().unwrap();
}

#[test]
fn a_subagent_set_aside_interrupted_before_its_commit_still_rebuilds() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    fs::write(
        task_dir.join("subagents/catalog.json"),
        serde_json::to_vec(&json!({ "schemaVersion": 99, "entries": [] })).unwrap(),
    )
    .unwrap();
    // The Chat itself is readable, and the newer catalog is already gone.
    interrupt_before_commit(&task_dir, "subagents");

    let store = open(root.path());
    assert_rebuilt(&store, &task_dir);
    let record: Value =
        serde_json::from_slice(&fs::read(task_dir.join("set-aside/set-aside.json")).unwrap())
            .unwrap();
    assert_eq!(record["part"], "subagent_catalog");
    assert_eq!(record["version"], 99);
    store.shutdown().unwrap();
}

#[test]
fn a_set_aside_interrupted_after_its_commit_finishes_on_the_next_load() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    set_snapshot_version(&task_dir, CHAT_SCHEMA_VERSION + 1);
    let store = open(root.path());
    store.load(TASK_ID).unwrap();
    store.shutdown().unwrap();
    // The crash came after the commit and before the pending directory became
    // the set-aside.
    fs::rename(
        task_dir.join("set-aside"),
        task_dir.join("set-aside.pending"),
    )
    .unwrap();

    let store = open(root.path());
    let loaded = store.load(TASK_ID).unwrap();
    assert!(loaded.messages.is_empty());
    assert!(task_dir.join("set-aside/chat.log").is_file());
    assert!(task_dir.join("set-aside/images/image_1").is_file());
    assert!(!task_dir.join("set-aside.pending").exists());
    store.shutdown().unwrap();
}

#[test]
fn a_rebuilt_chat_continues_the_frame_numbers_of_the_set_aside_one() {
    let root = TempDir::new().unwrap();
    let task_dir = stored_task(root.path());
    commit_frame(&task_dir, CHAT_SCHEMA_VERSION + 1);
    let before = metadata(&task_dir)["chatFrame"].as_u64().unwrap();

    let store = open(root.path());
    store.load(TASK_ID).unwrap();
    assert_eq!(store.chat_version(TASK_ID).unwrap(), before + 1);
    assert!(matches!(
        store
            .chat_changes_since(TASK_ID, before, before + 1)
            .unwrap(),
        super::super::ChatChangesSince::TooOld { .. }
    ));
    store.shutdown().unwrap();
}
