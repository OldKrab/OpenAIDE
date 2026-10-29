use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::super::{
    CommittedSubagentChange, SubagentCapabilitiesRecord, SubagentOperation, SubagentRecord,
    SubagentStatusRecord, SubagentWrite,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole, NormalizedMessage};
use crate::storage::Store;

fn spawn(store: &Store, native_session_id: &str) -> SubagentRecord {
    store
        .record_subagent_spawn(
            "task_1",
            "root-session",
            native_session_id,
            "Researcher".to_string(),
            "Inspect".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap()
}

fn chunk(native_session_id: &str, text: &str) -> SubagentWrite {
    SubagentWrite {
        native_session_id: native_session_id.to_string(),
        operation: SubagentOperation::AppendMessagePart(NormalizedMessage::AgentMessage {
            id: "reply".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: text.to_string(),
            }],
            created_at: "1".to_string(),
        }),
    }
}

fn tasks_root(root: &Path) -> PathBuf {
    root.join("task-store-v1/tasks")
}

fn subagent_root(root: &Path) -> PathBuf {
    tasks_root(root).join("task_1/subagents")
}

fn disk_catalog_revision(root: &Path) -> u64 {
    let bytes = std::fs::read(subagent_root(root).join("catalog.json")).unwrap();
    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["revision"]
        .as_u64()
        .unwrap()
}

#[test]
fn one_batch_applies_chunks_in_order_and_appends_each_history_once() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn(&store, "child");
    let state = store.task_journal().subagents();
    let writes = [
        chunk("child", "a"),
        chunk("child", "b"),
        chunk("child", "c"),
    ];

    let (committed, outcomes) = state.commit_batch(
        &tasks_root(temp.path()),
        "task_1",
        &writes.iter().collect::<Vec<_>>(),
    );

    assert!(outcomes.iter().all(Result::is_ok));
    let committed = committed.expect("the batch changed the history");
    let revisions = committed
        .revisions
        .iter()
        .map(|revision| (revision.revision, revision.change.clone()))
        .collect::<Vec<_>>();
    assert!(matches!(
        &revisions[..],
        [
            (1, Some(CommittedSubagentChange::Append(_))),
            (2, Some(CommittedSubagentChange::AppendText { text: b, .. })),
            (3, Some(CommittedSubagentChange::AppendText { text: c, .. })),
        ] if b == "b" && c == "c"
    ));
    let journal = std::fs::read_to_string(
        subagent_root(temp.path())
            .join(&record.subagent_id)
            .join("history.journal"),
    )
    .unwrap();
    assert_eq!(journal.lines().count(), 3, "one frame per revision");

    // A later chunk changes only the history; the catalog file keeps its revision.
    let persisted = disk_catalog_revision(temp.path());
    let (committed, _) =
        state.commit_batch(&tasks_root(temp.path()), "task_1", &[&chunk("child", "d")]);
    assert_eq!(disk_catalog_revision(temp.path()), persisted);
    assert!(committed.unwrap().catalog.revision > persisted);
    let history = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if matches!(&parts[..], [AgentMessagePart::Text { text }] if text == "abcd")
    ));
}

#[test]
fn a_failed_history_leaves_the_rest_of_its_batch_committed() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let broken = spawn(&store, "broken");
    let healthy = spawn(&store, "healthy");
    let journal = subagent_root(temp.path())
        .join(&broken.subagent_id)
        .join("history.journal");
    std::fs::remove_file(&journal).unwrap();
    std::fs::create_dir(&journal).unwrap();

    let writes = [
        chunk("broken", "lost"),
        chunk("healthy", "kept"),
        chunk("unknown", "nobody"),
    ];
    let (committed, outcomes) = store.task_journal().subagents().commit_batch(
        &tasks_root(temp.path()),
        "task_1",
        &writes.iter().collect::<Vec<_>>(),
    );

    assert!(outcomes[0].is_err());
    assert!(outcomes[1].is_ok());
    assert!(matches!(outcomes[2], Err(RuntimeError::TaskNotFound(_))));
    let committed = committed.unwrap();
    assert_eq!(
        committed
            .revisions
            .iter()
            .map(|revision| revision.subagent_id.as_str())
            .collect::<Vec<_>>(),
        [healthy.subagent_id.as_str()],
        "the failed history's revision is not published"
    );
    let entry = |id: &str| {
        committed
            .catalog
            .entries
            .iter()
            .find(|entry| entry.subagent_id == id)
            .unwrap()
            .clone()
    };
    assert!(!entry(&broken.subagent_id).history_available);
    assert!(entry(&healthy.subagent_id).history_available);
    assert!(store
        .subagent_history("task_1", &broken.subagent_id)
        .is_err());
    assert_eq!(
        store
            .subagent_history("task_1", &healthy.subagent_id)
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn a_failed_catalog_write_keeps_the_catalog_revision_rising() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    spawn(&store, "child");
    store
        .append_subagent_message_part("task_1", "child", chunk_message("streamed"))
        .unwrap()
        .wait()
        .unwrap();
    let published = store.subagent_catalog("task_1").unwrap().revision;
    let root = subagent_root(temp.path());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();

    let failed = store.update_subagent_status("task_1", "child", SubagentStatusRecord::Completed);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(failed.is_err());
    let catalog = store.subagent_catalog("task_1").unwrap();
    assert!(
        catalog.revision > published,
        "a client ignores a catalog older than one it holds"
    );
    assert_eq!(
        catalog.entries[0].status,
        SubagentStatusRecord::Running,
        "memory follows the catalog the disk kept"
    );
    let record = store
        .update_subagent_status("task_1", "child", SubagentStatusRecord::Completed)
        .unwrap();
    assert_eq!(record.status, SubagentStatusRecord::Completed);
    assert!(store.subagent_catalog("task_1").unwrap().revision > catalog.revision);
}

#[test]
fn a_barrier_commits_after_the_streamed_writes_before_it() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn(&store, "child");
    for text in ["one", " two"] {
        // The receipt is dropped, as the session sink does.
        store
            .append_subagent_message_part("task_1", "child", chunk_message(text))
            .unwrap();
    }

    let completed = store
        .update_subagent_status("task_1", "child", SubagentStatusRecord::Completed)
        .unwrap();

    assert_eq!(completed.history_revision, 2);
    // A stopped Subagent's history leaves memory; the disk holds both chunks.
    let history = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 2);
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if matches!(&parts[..], [AgentMessagePart::Text { text }] if text == "one two")
    ));
}

fn chunk_message(text: &str) -> NormalizedMessage {
    let SubagentOperation::AppendMessagePart(message) = chunk("child", text).operation else {
        unreachable!("chunk builds a message part");
    };
    message
}
