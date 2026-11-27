//! Chat frame numbering, the frames-after-X read, and the compaction crash windows.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::crash_recovery::accept_user_message;
use super::{task_projection, TaskJournalStore};
use crate::storage::task_journal::chat_log::ChatChangesSince;
use crate::storage::task_journal::model::TaskOperation;
use crate::storage::task_journal::{TaskProjection, TaskWrite};

const TASK: &str = "task_numbering";

fn open_with_task(root: &Path) -> (TaskJournalStore, TaskProjection) {
    let (store, _) = TaskJournalStore::open(root.to_path_buf()).unwrap();
    let projection = task_projection(TASK);
    store
        .submit(TaskWrite::barrier_create(projection.clone()))
        .unwrap()
        .wait()
        .unwrap();
    (store, projection)
}

fn task_dir(root: &Path) -> PathBuf {
    root.join("task-store-v1/tasks").join(TASK)
}

fn compact(store: &TaskJournalStore) {
    store
        .submit(TaskWrite::compaction_barrier(TASK))
        .unwrap()
        .wait()
        .unwrap();
}

fn replace_messages(store: &TaskJournalStore, projection: &TaskProjection) {
    store
        .submit(TaskWrite::barrier_operations_with_artifacts(
            TASK.to_string(),
            vec![TaskOperation::ReplaceMessages {
                messages: projection.messages.clone(),
                message_meta: Box::new(projection.message_meta.clone()),
            }],
            Vec::new(),
            Vec::new(),
        ))
        .unwrap()
        .wait()
        .unwrap();
}

#[test]
fn frame_numbers_keep_counting_across_compaction_replace_and_restart() {
    let root = TempDir::new().unwrap();
    let (store, mut projection) = open_with_task(root.path());
    let first = store.chat_version(TASK).unwrap();
    accept_user_message(&store, &mut projection, "one");
    accept_user_message(&store, &mut projection, "two");
    assert_eq!(store.chat_version(TASK).unwrap(), first + 2);

    compact(&store);
    assert_eq!(
        store.chat_version(TASK).unwrap(),
        first + 2,
        "compaction commits no frame"
    );
    accept_user_message(&store, &mut projection, "three");
    assert_eq!(store.chat_version(TASK).unwrap(), first + 3);

    // Replace is one frame, and it does not restart the count.
    projection.messages.truncate(1);
    projection.message_meta.message_count = 1;
    projection.message_meta.version += 1;
    replace_messages(&store, &projection);
    assert_eq!(store.chat_version(TASK).unwrap(), first + 4);
    compact(&store);
    accept_user_message(&store, &mut projection, "four");
    assert_eq!(store.chat_version(TASK).unwrap(), first + 5);
    store.shutdown().unwrap();

    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(reopened.chat_version(TASK).unwrap(), first + 5);
    reopened.shutdown().unwrap();
}

#[test]
fn frames_after_a_version_report_current_deltas_too_old_and_ahead() {
    let root = TempDir::new().unwrap();
    let (store, mut projection) = open_with_task(root.path());
    let base = store.chat_version(TASK).unwrap();
    accept_user_message(&store, &mut projection, "one");
    accept_user_message(&store, &mut projection, "two");
    let head = base + 2;

    assert!(matches!(
        store.chat_changes_since(TASK, head).unwrap(),
        ChatChangesSince::Current { version } if version == head
    ));
    match store.chat_changes_since(TASK, base).unwrap() {
        ChatChangesSince::Deltas { version, frames } => {
            assert_eq!(version, head);
            assert_eq!(
                frames.iter().map(|frame| frame.version).collect::<Vec<_>>(),
                [base + 1, base + 2]
            );
            assert!(frames.iter().all(|frame| !frame.operations.is_empty()));
        }
        other => panic!("expected deltas, got {other:?}"),
    }
    assert!(matches!(
        store.chat_changes_since(TASK, head + 5).unwrap(),
        ChatChangesSince::Ahead { version } if version == head
    ));

    // Compaction folds the frames into the snapshot: older clients reload.
    compact(&store);
    assert!(matches!(
        store.chat_changes_since(TASK, base).unwrap(),
        ChatChangesSince::TooOld { version } if version == head
    ));
    assert!(matches!(
        store.chat_changes_since(TASK, head).unwrap(),
        ChatChangesSince::Current { .. }
    ));
    accept_user_message(&store, &mut projection, "three");
    assert!(matches!(
        store.chat_changes_since(TASK, head).unwrap(),
        ChatChangesSince::Deltas { frames, .. } if frames.len() == 1
    ));

    // A Replace is not a delta: clients older than it reload, newer ones do not.
    projection.messages.truncate(1);
    projection.message_meta.message_count = 1;
    projection.message_meta.version += 1;
    replace_messages(&store, &projection);
    assert!(matches!(
        store.chat_changes_since(TASK, head).unwrap(),
        ChatChangesSince::TooOld { .. }
    ));
    assert!(matches!(
        store.chat_changes_since(TASK, head + 2).unwrap(),
        ChatChangesSince::Current { .. }
    ));
    store.shutdown().unwrap();

    // The answer is the same after a restart.
    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert!(matches!(
        reopened.chat_changes_since(TASK, head + 2).unwrap(),
        ChatChangesSince::Current { .. }
    ));
    assert!(matches!(
        reopened.chat_changes_since(TASK, head + 1).unwrap(),
        ChatChangesSince::TooOld { .. }
    ));
    reopened.shutdown().unwrap();
}

#[test]
fn a_crash_before_the_compaction_rename_keeps_the_log_and_cleans_the_orphan_temp() {
    let root = TempDir::new().unwrap();
    let (store, mut projection) = open_with_task(root.path());
    accept_user_message(&store, &mut projection, "one");
    accept_user_message(&store, &mut projection, "two");
    let version = store.chat_version(TASK).unwrap();
    store.shutdown().unwrap();
    let dir = task_dir(root.path());
    let log = std::fs::read(dir.join("chat.log")).unwrap();
    std::fs::write(dir.join("chat.log.compact"), b"half-written compaction").unwrap();

    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert!(!dir.join("chat.log.compact").exists());
    assert_eq!(std::fs::read(dir.join("chat.log")).unwrap(), log);
    assert_eq!(reopened.chat_version(TASK).unwrap(), version);
    assert_eq!(reopened.load(TASK).unwrap().messages.len(), 2);
    reopened.shutdown().unwrap();
}

#[test]
fn a_crash_after_the_compaction_rename_continues_numbering_from_the_snapshot() {
    let root = TempDir::new().unwrap();
    let (store, mut projection) = open_with_task(root.path());
    accept_user_message(&store, &mut projection, "one");
    accept_user_message(&store, &mut projection, "two");
    let version = store.chat_version(TASK).unwrap();
    compact(&store);
    // The process dies right after the rename: task.json is untouched, the
    // compacted log (no deltas) is the only Chat file.
    store.shutdown().unwrap();

    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(reopened.chat_version(TASK).unwrap(), version);
    let mut loaded = reopened.load(TASK).unwrap();
    assert_eq!(loaded.messages.len(), 2);
    accept_user_message(&reopened, &mut loaded, "three");
    assert_eq!(reopened.chat_version(TASK).unwrap(), version + 1);
    reopened.shutdown().unwrap();
}

#[test]
fn startup_scans_a_task_only_when_its_checkpoint_does_not_match_its_log() {
    let root = TempDir::new().unwrap();
    // Its own Task id: the log capture is shared by tests running in parallel.
    let task = "task_numbering_scan";
    let (store, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut projection = task_projection(task);
    store
        .submit(TaskWrite::barrier_create(projection.clone()))
        .unwrap()
        .wait()
        .unwrap();
    accept_user_message(&store, &mut projection, "one");
    store.shutdown().unwrap();
    let scans = |logs: &crate::logging::TestLogCapture| {
        logs.snapshot()
            .iter()
            .filter(|entry| entry["event"] == "task_metadata_recovery_started")
            .filter(|entry| entry["fields"]["task_id"] == task)
            .count()
    };

    let logs = crate::logging::capture_test_logs();
    let (clean, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    clean.shutdown().unwrap();
    assert_eq!(scans(&logs), 0, "a matching checkpoint costs no scan");

    // A stale checkpoint (the log is one frame ahead) triggers exactly one scan,
    // after which the checkpoint matches again.
    let metadata_path = root
        .path()
        .join("task-store-v1/tasks")
        .join(task)
        .join("task.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    let frame = metadata["chatFrame"].as_u64().unwrap();
    metadata["chatFrame"] = serde_json::json!(frame - 1);
    std::fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let (stale, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(scans(&logs), 1);
    // The capture drains on each read, so later counts are new scans only.
    assert_eq!(stale.chat_version(task).unwrap(), frame);
    stale.shutdown().unwrap();
    let (again, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(
        scans(&logs),
        0,
        "the repaired checkpoint is not scanned again"
    );
    again.shutdown().unwrap();
}

#[test]
fn a_checkpoint_beyond_the_log_fails_closed_instead_of_inventing_frames() {
    let root = TempDir::new().unwrap();
    let (store, mut projection) = open_with_task(root.path());
    accept_user_message(&store, &mut projection, "one");
    store.shutdown().unwrap();
    let metadata_path = task_dir(root.path()).join("task.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    let frame = metadata["chatFrame"].as_u64().unwrap();
    metadata["chatFrame"] = serde_json::json!(frame + 3);
    std::fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();

    let (reopened, _) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert!(reopened.load(TASK).is_err());
    reopened.shutdown().unwrap();
}
