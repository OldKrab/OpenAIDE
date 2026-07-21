use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, path::Path};

use tempfile::TempDir;

use super::{journal_path, TaskJournalStore};
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{
    MessageMeta, TaskConfigMutationState, TaskLifecycle, TaskPreparationRecord, TaskRecord,
};
use crate::storage::task_journal::frame::{FaultInjector, FaultPoint, JournalKind};
use crate::storage::task_journal::model::{JournalFrame, TaskOperation};
use crate::storage::task_journal::{TaskProjection, TaskWrite};

#[test]
fn worker_panic_resolves_receipt_and_emits_the_sole_root_fatal_signal() {
    let root = TempDir::new().expect("create state root");
    let faults = Arc::new(FaultInjector::armed(
        JournalKind::Task,
        FaultPoint::WorkerDispatch,
    ));
    let (store, _commits) =
        TaskJournalStore::open_with_faults(root.path().to_path_buf(), faults.clone())
            .expect("open store");
    let fatal_events = store.take_fatal_events();

    let receipt = store
        .submit(TaskWrite::barrier("task_1"))
        .expect("admit work before injected worker failure");
    let error = receipt
        .wait()
        .expect_err("worker death must fail its receipt");
    assert!(error.to_string().contains("worker stopped before commit"));
    assert!(!faults.pending(), "worker fault was not reached");

    let fatal = fatal_events
        .recv_timeout(Duration::from_secs(1))
        .expect("process supervisor receives root-wide failure");
    assert_eq!(fatal.reason, "worker_panicked");
    assert!(
        store.submit(TaskWrite::barrier("task_2")).is_err(),
        "dead worker must reject new admission"
    );
    assert!(
        fatal_events
            .recv_timeout(Duration::from_millis(20))
            .is_err(),
        "one worker death emits exactly one fatal signal"
    );
}

#[test]
fn journal_root_creation_requires_durable_parent_entries() {
    let root = TempDir::new().expect("create state root");
    let faults = Arc::new(FaultInjector::armed(
        JournalKind::Root,
        FaultPoint::DirectoryParentSync,
    ));

    let opened = TaskJournalStore::open_with_faults(root.path().to_path_buf(), faults.clone());
    assert!(
        opened.is_err(),
        "root directory sync failure must fail Store open"
    );

    assert!(
        !faults.pending(),
        "root directory sync boundary was not reached"
    );

    let retry_faults = Arc::new(FaultInjector::armed(
        JournalKind::Root,
        FaultPoint::DirectoryParentSync,
    ));
    let retried =
        TaskJournalStore::open_with_faults(root.path().to_path_buf(), retry_faults.clone());
    assert!(
        retried.is_err(),
        "retry must confirm directory entries left by the failed open"
    );
    assert!(
        !retry_faults.pending(),
        "retry bypassed the root directory sync boundary"
    );
}

#[test]
fn every_task_append_fault_freezes_the_task_across_restart() {
    for point in [
        FaultPoint::AppendOpen,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
    ] {
        assert_commit_fault_quarantines(JournalKind::Task, point, |store| {
            let mut task = store.load("task_1").expect("load setup Task").task;
            task.updated_at = "2026-07-21T00:00:00Z".to_string();
            TaskWrite::barrier_replace_task(task)
        });
    }
}

#[test]
fn every_task_create_fault_freezes_the_task_across_restart() {
    for point in [
        FaultPoint::DirectoryParentSync,
        FaultPoint::CreateOpen,
        FaultPoint::CreateHeaderWrite,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
        FaultPoint::ParentSync,
    ] {
        let root = TempDir::new().expect("create state root");
        let faults = Arc::new(FaultInjector::armed(JournalKind::Task, point));
        let (store, _commits) =
            TaskJournalStore::open_with_faults(root.path().to_path_buf(), faults.clone())
                .expect("open with deterministic create fault");
        store
            .submit(TaskWrite::barrier_create(task_projection("task_1")))
            .expect("admit Task create")
            .wait()
            .expect_err("armed Task create boundary must fail");
        assert!(!faults.pending(), "Task/{point:?} was not reached");
        assert!(store.load("task_1").is_err());
        store.shutdown().expect("worker remains healthy");

        let (reopened, _commits) =
            TaskJournalStore::open(root.path().to_path_buf()).expect("state root remains openable");
        assert!(
            reopened.load("task_1").is_err(),
            "Task/{point:?} quarantine must survive restart"
        );
        reopened.shutdown().expect("close verification store");
    }
}

#[test]
fn every_artifact_prepare_and_visibility_fault_freezes_across_restart() {
    for point in [
        FaultPoint::DirectoryParentSync,
        FaultPoint::CreateOpen,
        FaultPoint::CreateHeaderWrite,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
        FaultPoint::ParentSync,
    ] {
        assert_commit_fault_quarantines(JournalKind::Artifact, point, |_| terminal_write());
    }
    for point in [
        FaultPoint::AppendOpen,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
    ] {
        assert_existing_artifact_append_fault_quarantines(point);
    }
    for point in [
        FaultPoint::AppendOpen,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
    ] {
        assert_commit_fault_quarantines(JournalKind::ArtifactReference, point, |_| {
            terminal_write()
        });
    }
}

fn assert_existing_artifact_append_fault_quarantines(point: FaultPoint) {
    let root = TempDir::new().expect("create state root");
    let (setup, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    setup
        .submit(TaskWrite::barrier_create(task_projection("task_1")))
        .unwrap()
        .wait()
        .unwrap();
    setup.submit(terminal_write()).unwrap().wait().unwrap();
    setup.shutdown().unwrap();

    let faults = Arc::new(FaultInjector::armed(JournalKind::Artifact, point));
    let (store, _commits) =
        TaskJournalStore::open_with_faults(root.path().to_path_buf(), faults.clone()).unwrap();
    store
        .submit(terminal_write())
        .unwrap()
        .wait()
        .expect_err("armed artifact append boundary must fail");
    assert!(!faults.pending(), "Artifact/{point:?} was not reached");
    store.shutdown().unwrap();

    let (reopened, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert!(reopened.load("task_1").is_err());
    reopened.shutdown().unwrap();
}

#[test]
fn every_compaction_publication_fault_freezes_across_restart() {
    for point in [
        FaultPoint::CreateOpen,
        FaultPoint::CreateHeaderWrite,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
        FaultPoint::ParentSync,
        FaultPoint::CompactionValidate,
        FaultPoint::CompactionPublish,
        FaultPoint::CompactionPublishParentSync,
    ] {
        assert_commit_fault_quarantines(JournalKind::Compaction, point, |_| {
            TaskWrite::compaction_barrier("task_1")
        });
    }
}

#[test]
fn replay_rejects_operations_that_live_validation_would_reject() {
    let root = TempDir::new().expect("create state root");
    let tasks_root = root.path().join("task-store-v1/tasks");
    std::fs::create_dir_all(&tasks_root).expect("create tasks root");
    let mut projection = task_projection("different_task");
    projection.message_meta.task_id = "different_task".to_string();
    let frame = JournalFrame {
        format_version: 1,
        sequence: 1,
        operations: vec![TaskOperation::Create {
            projection: Box::new(projection),
        }],
    };
    let path = journal_path(&tasks_root, "task_1").expect("journal path");
    crate::storage::task_journal::frame::create(&path, &frame)
        .expect("write semantically invalid frame");

    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();

    assert!(store.load("task_1").is_err());
    store.shutdown().unwrap();
}

#[test]
fn corrupt_artifact_is_fenced_before_a_later_write() {
    assert_unavailable_artifact_rejects_later_write(|path| {
        use std::io::{Read, Seek, Write};
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        file.seek(std::io::SeekFrom::End(-1)).unwrap();
        let mut byte = [0];
        file.read_exact(&mut byte).unwrap();
        file.seek(std::io::SeekFrom::End(-1)).unwrap();
        file.write_all(&[byte[0] ^ 0xff]).unwrap();
        file.sync_all().unwrap();
    });
}

#[test]
fn physically_short_artifact_is_fenced_before_a_later_write() {
    assert_unavailable_artifact_rejects_later_write(|path| {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_len(10).unwrap();
        file.sync_all().unwrap();
    });
}

fn assert_unavailable_artifact_rejects_later_write(damage: impl FnOnce(&Path)) {
    let root = TempDir::new().expect("create state root");
    let (setup, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    setup
        .submit(TaskWrite::barrier_create(task_projection("task_1")))
        .unwrap()
        .wait()
        .unwrap();
    setup.submit(terminal_write()).unwrap().wait().unwrap();
    setup.shutdown().unwrap();
    let path = root
        .path()
        .join("task-store-v1/tasks/task_1/artifacts/artifact_1.journal");
    damage(&path);
    let damaged = std::fs::read(&path).unwrap();

    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(terminal_write())
        .unwrap()
        .wait()
        .expect_err("unavailable artifact must reject later writes");

    assert_eq!(std::fs::read(path).unwrap(), damaged);
    assert!(
        store.load("task_1").is_err(),
        "failed write freezes its Task"
    );
    store.shutdown().unwrap();
}

fn assert_commit_fault_quarantines(
    kind: JournalKind,
    point: FaultPoint,
    write: impl FnOnce(&TaskJournalStore) -> TaskWrite,
) {
    let root = TempDir::new().expect("create state root");
    create_setup_task(root.path());
    let faults = Arc::new(FaultInjector::armed(kind, point));
    let (store, _commits) =
        TaskJournalStore::open_with_faults(root.path().to_path_buf(), faults.clone())
            .expect("reopen with deterministic fault");

    store
        .submit(write(&store))
        .expect("admit write")
        .wait()
        .expect_err("armed durability boundary must fail");
    assert!(!faults.pending(), "{kind:?}/{point:?} was not reached");
    assert!(store.load("task_1").is_err(), "failed Task must freeze");
    store.shutdown().expect("unrelated worker remains healthy");

    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("state root remains openable");
    assert!(
        reopened.load("task_1").is_err(),
        "{kind:?}/{point:?} quarantine must survive restart"
    );
    reopened.shutdown().expect("close verification store");
}

fn create_setup_task(root: &Path) {
    let (store, _commits) = TaskJournalStore::open(root.to_path_buf()).expect("open setup store");
    store
        .submit(TaskWrite::barrier_create(task_projection("task_1")))
        .expect("admit setup Task")
        .wait()
        .expect("commit setup Task");
    store.shutdown().expect("close setup store");
}

fn terminal_write() -> TaskWrite {
    TaskWrite::stream_append_terminal("task_1", "artifact_1", "terminal_1", "accepted bytes")
}

fn task_projection(task_id: &str) -> TaskProjection {
    TaskProjection {
        task: TaskRecord {
            task_id: task_id.to_string(),
            title: None,
            status: TaskStatus::Inactive,
            task_version: 1,
            message_history_version: 0,
            unread: false,
            attention: None,
            created_at: "2026-07-20T00:00:00Z".to_string(),
            updated_at: "2026-07-20T00:00:00Z".to_string(),
            last_activity: "2026-07-20T00:00:00Z".to_string(),
            agent_id: "agent_1".to_string(),
            agent_name: "Agent".to_string(),
            isolation: IsolationKind::Local,
            workspace_root: "/workspace".to_string(),
            project_root: Some("/workspace".to_string()),
            worktree_id: None,
            lifecycle: TaskLifecycle::Visible,
            agent_session_id: Some("session_1".to_string()),
            active_turn_id: None,
            active_turn_started_at: None,
            archived: false,
            tombstoned: false,
            revision: 1,
            config_options_catalog: None,
            config_mutation: TaskConfigMutationState::default(),
            agent_commands_catalog: None,
            model_id: None,
            supports_image_input: false,
            preparation: TaskPreparationRecord::Ready,
        },
        messages: Vec::new(),
        message_meta: MessageMeta {
            task_id: task_id.to_string(),
            version: 0,
            message_count: 0,
            local_history_updated_at: "2026-07-20T00:00:00Z".to_string(),
            first_cursor: None,
            last_cursor: None,
        },
        artifact_heads: HashMap::new(),
    }
}
