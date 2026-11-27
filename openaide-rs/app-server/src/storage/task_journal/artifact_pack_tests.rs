//! Artifact packs at the store boundary: a creating commit's initial Tool artifacts share one
//! file, and every later operation behaves as it does for a per-artifact file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use super::{task_projection, TaskJournalStore};
use crate::protocol::model::{ActivityToolContent, ActivityToolDetails};
use crate::storage::records::StoredMessage;
use crate::storage::task_journal::frame::{FaultInjector, FaultPoint, JournalKind};
use crate::storage::task_journal::model::ToolArtifactReplacement;
use crate::storage::task_journal::TaskWrite;

const TASK: &str = "task_1";

fn replacement(index: usize, text: &str) -> ToolArtifactReplacement {
    ToolArtifactReplacement {
        artifact_id: format!("artifact_{index}"),
        details: ActivityToolDetails {
            locations: Vec::new(),
            content: vec![ActivityToolContent::Text {
                text: text.to_string(),
            }],
            input: None,
            output: None,
        },
    }
}

fn replacements(count: usize) -> Vec<ToolArtifactReplacement> {
    (0..count)
        .map(|index| replacement(index, &format!("output {index}")))
        .collect()
}

fn artifacts_dir(root: &Path) -> PathBuf {
    root.join("task-store-v1/tasks")
        .join(TASK)
        .join("artifacts")
}

/// File names in the Task's artifacts directory, sorted.
fn artifact_files(root: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(artifacts_dir(root))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn pack_files(root: &Path) -> Vec<String> {
    artifact_files(root)
        .into_iter()
        .filter(|name| name.ends_with(".pack"))
        .collect()
}

fn journal_files(root: &Path) -> Vec<String> {
    artifact_files(root)
        .into_iter()
        .filter(|name| name.ends_with(".journal"))
        .collect()
}

fn open(root: &Path) -> TaskJournalStore {
    TaskJournalStore::open(root.to_path_buf())
        .expect("open journal")
        .0
}

fn create(store: &TaskJournalStore, artifacts: Vec<ToolArtifactReplacement>) {
    store
        .submit(TaskWrite::barrier_create_with_artifacts(
            task_projection(TASK),
            artifacts,
        ))
        .unwrap()
        .wait()
        .unwrap();
}

fn replace_details(store: &TaskJournalStore, replacement: ToolArtifactReplacement) {
    let projection = store.load(TASK).unwrap();
    store
        .submit(TaskWrite::barrier_replace_projection(
            projection,
            vec![replacement],
        ))
        .unwrap()
        .wait()
        .unwrap();
}

fn text_of(store: &TaskJournalStore, index: usize) -> String {
    let loaded = store
        .load_tool_artifact(TASK, &format!("artifact_{index}"))
        .unwrap();
    match &loaded.details.unwrap().content[0] {
        ActivityToolContent::Text { text } => text.clone(),
        other => panic!("unexpected content {other:?}"),
    }
}

#[test]
fn a_creating_commit_packs_many_artifacts_and_reads_each_back() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    create(&store, replacements(60));

    assert_eq!(pack_files(root.path()).len(), 1);
    assert!(journal_files(root.path()).is_empty());
    for index in 0..60 {
        assert_eq!(text_of(&store, index), format!("output {index}"));
        let loaded = store
            .load_tool_artifact(TASK, &format!("artifact_{index}"))
            .unwrap();
        assert_eq!(loaded.revision, 1);
    }
    store.shutdown().unwrap();

    let reopened = open(root.path());
    assert_eq!(text_of(&reopened, 17), "output 17");
    reopened.shutdown().unwrap();
}

#[test]
fn a_creating_commit_with_one_artifact_keeps_its_own_file() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    create(&store, replacements(1));

    assert!(pack_files(root.path()).is_empty());
    assert_eq!(journal_files(root.path()), ["artifact_0.journal"]);
    assert_eq!(text_of(&store, 0), "output 0");
    store.shutdown().unwrap();
}

#[test]
fn later_operations_on_a_packed_artifact_keep_versions_and_appends_exact() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    create(&store, replacements(5));

    // Terminal output appended after the pack, then a details replacement.
    for data in ["a", "b"] {
        store
            .submit(TaskWrite::stream_append_terminal(
                TASK,
                "artifact_3",
                "terminal_1",
                data,
            ))
            .unwrap()
            .wait()
            .unwrap();
    }
    replace_details(&store, replacement(3, "replaced"));

    let (loaded, appends) = store
        .load_tool_artifact_since(TASK, "artifact_3", Some(1))
        .unwrap();
    assert_eq!(loaded.revision, 4);
    assert_eq!(loaded.terminal_outputs["terminal_1"], "ab");
    assert_eq!(
        appends.iter().map(|a| a.data.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(text_of(&store, 3), "replaced");
    // Only the touched artifact gained its own file; the rest still read from the pack.
    assert_eq!(journal_files(root.path()), ["artifact_3.journal"]);
    assert_eq!(text_of(&store, 2), "output 2");
    store.shutdown().unwrap();

    // The same after a restart: reconciliation sees the pack-only and the promoted artifacts.
    let reopened = open(root.path());
    let (loaded, appends) = reopened
        .load_tool_artifact_since(TASK, "artifact_3", Some(3))
        .unwrap();
    assert_eq!(loaded.revision, 4);
    assert_eq!(appends.len(), 0);
    assert_eq!(text_of(&reopened, 2), "output 2");
    reopened
        .submit(TaskWrite::stream_append_terminal(
            TASK,
            "artifact_2",
            "terminal_1",
            "c",
        ))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(
        reopened
            .load_tool_artifact(TASK, "artifact_2")
            .unwrap()
            .terminal_outputs["terminal_1"],
        "c"
    );
    reopened.shutdown().unwrap();
}

#[test]
fn per_artifact_files_and_packs_mix_in_one_store() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    create(&store, replacements(1));
    // A second commit creating several artifacts packs them beside the existing file.
    let projection = store.load(TASK).unwrap();
    store
        .submit(TaskWrite::barrier_replace_projection(
            projection,
            (1..5)
                .map(|index| replacement(index, &format!("later {index}")))
                .collect(),
        ))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(pack_files(root.path()).len(), 1);
    assert_eq!(journal_files(root.path()), ["artifact_0.journal"]);
    store.shutdown().unwrap();

    let reopened = open(root.path());
    assert_eq!(text_of(&reopened, 0), "output 0");
    for index in 1..5 {
        assert_eq!(text_of(&reopened, index), format!("later {index}"));
    }
    reopened.shutdown().unwrap();
}

#[test]
fn forced_compaction_leaves_packed_artifacts_readable() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    create(&store, replacements(4));
    replace_details(&store, replacement(1, "second"));
    replace_details(&store, replacement(1, "third"));

    store
        .submit(TaskWrite::compaction_barrier(TASK))
        .unwrap()
        .wait()
        .unwrap();

    assert_eq!(text_of(&store, 0), "output 0");
    assert_eq!(text_of(&store, 1), "third");
    assert_eq!(pack_files(root.path()).len(), 1);
    store.shutdown().unwrap();
}

#[test]
fn purging_a_task_removes_its_packs() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    let mut projection = task_projection(TASK);
    projection.task.tombstoned = true;
    store
        .submit(TaskWrite::barrier_create_with_artifacts(
            projection,
            replacements(6),
        ))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(pack_files(root.path()).len(), 1);

    store.purge_tombstoned_task(TASK).unwrap();

    assert!(artifact_files(root.path()).is_empty());
    assert!(!artifacts_dir(root.path()).exists());
    store.shutdown().unwrap();
}

#[test]
fn syncs_for_a_creating_commit_do_not_grow_with_its_artifacts() {
    let syncs = |count: usize| {
        let root = TempDir::new().unwrap();
        let store = open(root.path());
        let baseline = store.durability_sync_calls();
        create(&store, replacements(count));
        let syncs = store.durability_sync_calls() - baseline;
        store.shutdown().unwrap();
        syncs
    };

    assert_eq!(syncs(10), syncs(500));
}

#[test]
fn an_undurable_pack_fails_the_commit_and_no_task_references_it() {
    // A failure at each pack boundary: nothing written yet, written but unsynced, and
    // synced but its directory entry unconfirmed. The commit fails before any Task frame
    // exists, so after a restart no Task loads and none lists as committed.
    for point in [
        FaultPoint::CreateOpen,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FileSync,
        FaultPoint::ParentSync,
    ] {
        let root = TempDir::new().unwrap();
        let faults = Arc::new(FaultInjector::armed(JournalKind::Artifact, point));
        let (store, _commits) =
            TaskJournalStore::open_with_faults(root.path().to_path_buf(), faults).unwrap();
        store
            .submit(TaskWrite::barrier_create_with_artifacts(
                task_projection(TASK),
                replacements(8),
            ))
            .unwrap()
            .wait()
            .expect_err("a pack that cannot be made durable fails the commit");
        let _ = store.shutdown();

        let reopened = open(root.path());
        assert!(
            reopened.load(TASK).is_err(),
            "{point:?}: a Task referencing an undurable pack was committed"
        );
        assert!(
            !root
                .path()
                .join("task-store-v1/tasks")
                .join(TASK)
                .join("chat.log")
                .exists(),
            "{point:?}: the Task's Chat was published before its artifacts were durable"
        );
        reopened.shutdown().unwrap();
    }
}

#[test]
fn a_pack_that_was_never_completed_reads_as_a_missing_artifact_and_is_logged() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    create(&store, replacements(6));
    store.shutdown().unwrap();
    let pack = artifacts_dir(root.path()).join(&pack_files(root.path())[0]);
    let bytes = std::fs::read(&pack).unwrap();
    std::fs::write(&pack, &bytes[..bytes.len() / 2]).unwrap();

    let logs = crate::logging::capture_test_logs();
    let reopened = open(root.path());
    reopened
        .load_tool_artifact(TASK, "artifact_2")
        .expect_err("a truncated pack cannot serve its artifacts");

    // The Task itself stays readable; only the Tool detail is unavailable.
    assert_eq!(reopened.load(TASK).unwrap().task.task_id, TASK);
    let failure = logs
        .snapshot()
        .into_iter()
        .find(|entry| {
            entry["event"] == "tool_artifact_pack_unavailable" && entry["fields"]["task_id"] == TASK
        })
        .expect("pack read failure is logged");
    assert_eq!(failure["fields"]["failure_kind"], "incomplete");
    assert!(failure["fields"].get("path").is_none());
    reopened.shutdown().unwrap();
}

#[test]
fn a_creating_commit_logs_its_pack_count_and_bytes() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    let logs = crate::logging::capture_test_logs();
    // Log capture is process-wide: a distinct Task id picks out this commit's event.
    let task_id = "task_logged_pack";
    store
        .submit(TaskWrite::barrier_create_with_artifacts(
            task_projection(task_id),
            replacements(12),
        ))
        .unwrap()
        .wait()
        .unwrap();

    let committed = logs
        .snapshot()
        .into_iter()
        .find(|entry| {
            entry["event"] == "task_journal_create_committed"
                && entry["fields"]["task_id"] == task_id
        })
        .expect("creating commit is logged");
    assert_eq!(committed["fields"]["created_artifact_pack_count"], 1);
    assert_eq!(committed["fields"]["created_artifact_file_count"], 0);
    let directory = root
        .path()
        .join("task-store-v1/tasks")
        .join(task_id)
        .join("artifacts");
    let pack = std::fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        committed["fields"]["created_artifact_pack_bytes"],
        std::fs::metadata(pack).unwrap().len()
    );
    store.shutdown().unwrap();
}

/// Local measurement, not an assertion on time: a 551-Tool-call, 1106-message adoption.
/// `cargo test -p openaide-app-server measure_creating_commit -- --ignored --nocapture`.
#[test]
#[ignore = "measurement"]
fn measure_creating_commit_with_551_artifacts() {
    let root = TempDir::new().unwrap();
    let store = open(root.path());
    let mut projection = task_projection(TASK);
    let body = "x".repeat(900);
    projection.messages = (1..=1106)
        .map(|sequence| StoredMessage {
            sequence,
            chat: crate::protocol::model::ChatMessage {
                cursor: sequence.to_string(),
                identity: format!("message_{sequence}"),
                message_type: "agent_message".to_string(),
                message_id: format!("message_{sequence}"),
                message: crate::protocol::model::NormalizedMessage::AgentMessage {
                    id: format!("message_{sequence}"),
                    role: crate::protocol::model::AgentMessageRole::Agent,
                    parts: vec![crate::protocol::model::AgentMessagePart::Text {
                        text: body.clone(),
                    }],
                    created_at: "2026-07-20T00:00:00Z".to_string(),
                },
                timing: Default::default(),
            },
        })
        .collect();
    projection.message_meta.message_count = 1106;
    projection.message_meta.version = 1106;
    let artifacts = (0..551)
        .map(|index| replacement(index, &"y".repeat(2000)))
        .collect();
    let logs = crate::logging::capture_test_logs();
    store
        .submit(TaskWrite::barrier_create_with_artifacts(
            projection, artifacts,
        ))
        .unwrap()
        .wait()
        .unwrap();
    for entry in logs.snapshot() {
        if entry["event"] == "task_journal_create_committed"
            || entry["event"] == "task_initial_files_published"
        {
            eprintln!("{} {}", entry["event"], entry["fields"]);
        }
    }
    store.shutdown().unwrap();
}
