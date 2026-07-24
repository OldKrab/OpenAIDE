use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Barrier};

use openaide_app_server::protocol::model::{
    AgentCommand, AgentCommandsCatalog, AgentMessagePart, AgentMessageRole, ChatMessage,
    ConfigOptionsCatalog, IsolationKind, NormalizedMessage, TaskStatus,
};
use openaide_app_server::storage::records::{
    MessageMeta, StoredMessage, TaskConfigMutationState, TaskLifecycle, TaskPreparationRecord,
    TaskRecord, TaskTitle, TaskTitleSource,
};
use openaide_app_server::storage::task_journal::{TaskJournalStore, TaskProjection, TaskWrite};
use tempfile::TempDir;

#[test]
fn a_committed_task_survives_store_restart() {
    let root = TempDir::new().expect("create state root");
    let projection = task_projection("task_1");

    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(projection))
        .expect("admit create")
        .wait()
        .expect("commit create");

    let committed = commits.recv().expect("receive committed batch");
    assert_eq!(committed.task_id, "task_1");
    assert_eq!(committed.journal_sequence, 1);
    store.shutdown().expect("close store");

    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("reopen store");
    let loaded = reopened.load("task_1").expect("load committed Task");

    assert_eq!(loaded.task.task_id, "task_1");
    assert_eq!(loaded.task.status, TaskStatus::Inactive);
    assert!(loaded.messages.is_empty());
    assert_eq!(loaded.message_meta.message_count, 0);
    reopened.shutdown().expect("close reopened store");
}

#[test]
fn durable_task_metadata_and_chat_survive_without_persisting_live_agent_catalogs() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_split_authority");
    projection
        .messages
        .push(agent_message("agent_message", "durable chat"));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    projection.task.agent_commands_catalog = Some(AgentCommandsCatalog {
        commands: vec![AgentCommand {
            name: "review".to_string(),
            description: "Review changes".to_string(),
            input_hint: None,
        }],
    });
    projection.task.config_options_catalog = Some(ConfigOptionsCatalog::empty("codex"));

    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    let mut task = store.load("task_split_authority").unwrap().task;
    task.title = openaide_app_server::storage::records::TaskTitleState::from_title(TaskTitle::new(
        "Durable title",
        TaskTitleSource::User,
    ));
    store
        .submit(TaskWrite::barrier_replace_task(task))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();

    let task_dir = root.path().join("task-store-v1/tasks/task_split_authority");
    assert!(task_dir.join("task.json").is_file());
    assert!(task_dir.join("chat.snapshot").is_file());
    assert!(!task_dir.join("task.journal").exists());

    let (reopened, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = reopened.load("task_split_authority").unwrap();
    assert_eq!(
        loaded.task.title.as_ref().map(TaskTitle::value),
        Some("Durable title")
    );
    assert_eq!(agent_text(&loaded.messages[0]), "durable chat");
    assert!(loaded.task.agent_commands_catalog.is_none());
    assert!(loaded.task.config_options_catalog.is_none());
    reopened.shutdown().unwrap();
}

#[test]
fn idle_compaction_merges_chat_deltas_into_the_materialized_snapshot() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_split_compact");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    for _ in 0..128 {
        store
            .submit(TaskWrite::stream_append_text(
                "task_split_compact",
                "agent_message",
                "x",
                "2026-07-22T00:00:00Z",
            ))
            .unwrap()
            .wait()
            .unwrap();
    }
    store
        .submit(TaskWrite::barrier("task_split_compact"))
        .unwrap()
        .wait()
        .unwrap();
    store
        .submit(TaskWrite::compaction_if_worthwhile_barrier(
            "task_split_compact",
        ))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();

    let task_dir = root.path().join("task-store-v1/tasks/task_split_compact");
    assert!(!task_dir.join("chat.journal").exists());
    let (reopened, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(
        agent_text(&reopened.load("task_split_compact").unwrap().messages[0]),
        "x".repeat(128)
    );
    reopened.shutdown().unwrap();
}

#[test]
fn chat_committed_before_metadata_is_repaired_after_restart() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_metadata_lag");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    store
        .submit(TaskWrite::stream_append_text(
            "task_metadata_lag",
            "agent_message",
            "durable",
            "1",
        ))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();

    let metadata_path = root
        .path()
        .join("task-store-v1/tasks/task_metadata_lag/task.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    metadata["chatSequence"] = serde_json::json!(0);
    metadata["storageSequence"] = serde_json::json!(1);
    std::fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();

    let (reopened, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = reopened.load("task_metadata_lag").unwrap();
    assert_eq!(agent_text(&loaded.messages[0]), "durable");
    reopened.shutdown().unwrap();

    let repaired: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metadata_path).unwrap()).unwrap();
    assert_eq!(repaired["chatSequence"], 1);
}

#[test]
fn stale_pre_compaction_chat_tail_is_ignored_after_snapshot_publication() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_stale_tail");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    store
        .submit(TaskWrite::stream_append_text(
            "task_stale_tail",
            "agent_message",
            "once",
            "1",
        ))
        .unwrap()
        .wait()
        .unwrap();
    let task_dir = root.path().join("task-store-v1/tasks/task_stale_tail");
    let stale_tail = std::fs::read(task_dir.join("chat.journal")).unwrap();
    store
        .submit(TaskWrite::compaction_barrier("task_stale_tail"))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();

    // Models a crash after publishing the new snapshot pointer but before the
    // obsolete generation was removed from the directory.
    std::fs::write(task_dir.join("chat.journal"), stale_tail).unwrap();
    let (reopened, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(
        agent_text(&reopened.load("task_stale_tail").unwrap().messages[0]),
        "once"
    );
    reopened.shutdown().unwrap();
}

#[test]
fn first_artifact_read_and_live_append_are_serialized_without_losing_bytes() {
    let root = TempDir::new().expect("create state root");
    let (setup, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    setup
        .submit(TaskWrite::barrier_create(task_projection(
            "task_artifact_race",
        )))
        .unwrap()
        .wait()
        .unwrap();
    let original = "x".repeat(1024 * 1024);
    setup
        .submit(TaskWrite::stream_append_terminal(
            "task_artifact_race",
            "artifact_1",
            "terminal_1",
            original.clone(),
        ))
        .unwrap()
        .wait()
        .unwrap();
    setup.shutdown().unwrap();

    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let start = Arc::new(Barrier::new(3));
    let reader_store = store.clone();
    let reader_start = start.clone();
    let reader = std::thread::spawn(move || {
        reader_start.wait();
        reader_store
            .load_tool_artifact("task_artifact_race", "artifact_1")
            .unwrap()
            .terminal_outputs["terminal_1"]
            .len()
    });
    let writer_store = store.clone();
    let writer_start = start.clone();
    let writer = std::thread::spawn(move || {
        writer_start.wait();
        writer_store
            .submit(TaskWrite::stream_append_terminal(
                "task_artifact_race",
                "artifact_1",
                "terminal_1",
                "y",
            ))
            .unwrap()
            .wait()
            .unwrap();
    });
    start.wait();
    let observed_len = reader.join().unwrap();
    writer.join().unwrap();
    assert!(observed_len == original.len() || observed_len == original.len() + 1);
    assert_eq!(
        store
            .load_tool_artifact("task_artifact_race", "artifact_1")
            .unwrap()
            .terminal_outputs["terminal_1"],
        format!("{original}y")
    );
    store.shutdown().unwrap();
}

#[test]
fn checksum_damage_isolated_to_the_affected_task() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_damaged");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(projection))
        .expect("admit create")
        .wait()
        .expect("commit create");
    store
        .submit(TaskWrite::stream_append_text(
            "task_damaged",
            "agent_message",
            "x",
            "1",
        ))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().expect("close store");

    corrupt_final_checksum(root.path(), "task_damaged");

    let (reopened, _commits) = TaskJournalStore::open(root.path().to_path_buf())
        .expect("open state root with one damaged Task");
    let error = reopened
        .load("task_damaged")
        .expect_err("damaged Task must be unavailable");

    assert!(error.to_string().contains("checksum mismatch"));
    reopened.shutdown().expect("close reopened store");
}

#[test]
fn an_incomplete_tail_is_discarded_before_the_next_commit() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_tail");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(projection))
        .expect("admit create")
        .wait()
        .expect("commit create");
    store
        .submit(TaskWrite::stream_append_text(
            "task_tail",
            "agent_message",
            "x",
            "1",
        ))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().expect("close store");
    append_incomplete_frame(root.path(), "task_tail");

    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("recover incomplete tail");
    let mut task = reopened
        .load("task_tail")
        .expect("load recovered Task")
        .task;
    task.status = TaskStatus::Completed;
    task.task_version += 1;
    let committed = reopened
        .submit(TaskWrite::barrier_replace_task(task))
        .expect("admit replacement")
        .wait()
        .expect("commit after recovered tail");
    assert_eq!(committed.journal_sequence, 3);
    reopened.shutdown().expect("close recovered store");

    let (verified, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("reopen verified store");
    assert_eq!(
        verified.load("task_tail").expect("load Task").task.status,
        TaskStatus::Completed
    );
    verified.shutdown().expect("close verified store");
}

#[test]
fn sequential_stream_chunks_are_batched_and_replay_exactly_once() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_stream");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;

    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(projection))
        .expect("admit create")
        .wait()
        .expect("commit create");

    for _ in 0..10_002 {
        store
            .submit(TaskWrite::stream_append_text(
                "task_stream",
                "agent_message",
                "x",
                "2026-07-20T00:00:01Z",
            ))
            .expect("admit streamed text");
    }
    let barrier = store
        .submit(TaskWrite::barrier("task_stream"))
        .expect("admit barrier")
        .wait()
        .expect("flush streamed text");

    assert!(barrier.journal_sequence <= 50, "unexpected batch count");
    assert!(commits.try_iter().count() <= 50, "unexpected publications");
    store.shutdown().expect("close store");

    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("reopen store");
    let loaded = reopened.load("task_stream").expect("load streamed Task");
    assert_eq!(agent_text(&loaded.messages[0]).len(), 10_002);
    assert!(agent_text(&loaded.messages[0])
        .bytes()
        .all(|byte| byte == b'x'));
    reopened.shutdown().expect("close reopened store");
}

#[test]
fn compaction_replaces_history_with_one_restart_safe_projection() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_compact");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    for _ in 0..1_000 {
        store
            .submit(TaskWrite::stream_append_text(
                "task_compact",
                "agent_message",
                "x",
                "2",
            ))
            .unwrap();
    }
    store
        .submit(TaskWrite::barrier("task_compact"))
        .unwrap()
        .wait()
        .unwrap();
    let journal = task_journal_path(root.path(), "task_compact");
    let before = journal.metadata().unwrap().len();

    store
        .submit(TaskWrite::compaction_barrier("task_compact"))
        .unwrap()
        .wait()
        .unwrap();
    assert!(before > 0);
    assert!(!journal.exists());
    store.shutdown().unwrap();

    let (reopened, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(
        agent_text(&reopened.load("task_compact").unwrap().messages[0]),
        "x".repeat(1_000)
    );
    reopened.shutdown().unwrap();
}

#[test]
fn idle_compaction_waits_for_a_measured_reclaim_threshold() {
    let root = TempDir::new().expect("create state root");
    let mut projection = task_projection("task_threshold");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    store
        .submit(TaskWrite::stream_append_text(
            "task_threshold",
            "agent_message",
            "x",
            "1",
        ))
        .unwrap()
        .wait()
        .unwrap();
    let journal = task_journal_path(root.path(), "task_threshold");
    let before_small_check = journal.metadata().unwrap().len();

    let skipped = store
        .submit(TaskWrite::compaction_if_worthwhile_barrier(
            "task_threshold",
        ))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(skipped.journal_sequence, 2);
    assert_eq!(journal.metadata().unwrap().len(), before_small_check);

    for version in 2..=128 {
        store
            .submit(TaskWrite::stream_append_text(
                "task_threshold",
                "agent_message",
                "x",
                version.to_string(),
            ))
            .unwrap()
            .wait()
            .unwrap();
    }
    let before_compaction = journal.metadata().unwrap().len();
    let compacted = store
        .submit(TaskWrite::compaction_if_worthwhile_barrier(
            "task_threshold",
        ))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(compacted.journal_sequence, 129);
    assert!(before_compaction > 0);
    assert!(!journal.exists());
    store.shutdown().unwrap();
}

#[test]
fn terminal_output_is_committed_lazily_without_revising_the_task() {
    let root = TempDir::new().expect("create state root");
    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(task_projection("task_terminal")))
        .expect("admit create")
        .wait()
        .expect("commit create");

    for _ in 0..10_002 {
        store
            .submit(TaskWrite::stream_append_terminal(
                "task_terminal",
                "artifact_execute_1",
                "terminal_1",
                "x",
            ))
            .expect("admit terminal output");
    }
    store
        .submit(TaskWrite::barrier("task_terminal"))
        .expect("admit terminal barrier")
        .wait()
        .expect("commit terminal output");

    let durable_batches = commits.try_iter().collect::<Vec<_>>();
    assert!(durable_batches.len() <= 50, "unexpected batch count");
    assert!(durable_batches[1..]
        .iter()
        .all(|batch| !batch.task_snapshot_changed));
    assert_eq!(store.load("task_terminal").unwrap().task.revision, 1);
    store.shutdown().expect("close store");

    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("reopen store");
    let artifact = reopened
        .load_tool_artifact("task_terminal", "artifact_execute_1")
        .expect("load lazy Tool artifact");
    assert_eq!(artifact.terminal_outputs["terminal_1"].len(), 10_002);
    assert!(artifact.terminal_outputs["terminal_1"]
        .bytes()
        .all(|byte| byte == b'x'));
    assert_eq!(reopened.load("task_terminal").unwrap().task.revision, 1);
    reopened.shutdown().expect("close reopened store");
}

#[test]
fn prepared_artifact_tail_without_task_reference_is_discarded() {
    let root = TempDir::new().expect("create state root");
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(task_projection("task_orphan")))
        .expect("admit create")
        .wait()
        .expect("commit create");
    store
        .submit(TaskWrite::stream_append_terminal(
            "task_orphan",
            "artifact_execute_1",
            "terminal_1",
            "x",
        ))
        .expect("admit terminal output");
    store
        .submit(TaskWrite::barrier("task_orphan"))
        .expect("admit barrier")
        .wait()
        .expect("commit terminal output");
    store.shutdown().expect("close store");

    append_complete_orphan_artifact_frame(root.path(), "task_orphan");

    let (recovered, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("reconcile orphan frame");
    assert_eq!(
        recovered
            .load_tool_artifact("task_orphan", "artifact_execute_1")
            .expect("load committed artifact")
            .terminal_outputs["terminal_1"],
        "x"
    );
    recovered
        .submit(TaskWrite::stream_append_terminal(
            "task_orphan",
            "artifact_execute_1",
            "terminal_1",
            "y",
        ))
        .expect("admit output after recovery");
    recovered
        .submit(TaskWrite::barrier("task_orphan"))
        .expect("admit recovery barrier")
        .wait()
        .expect("commit after recovery");
    recovered.shutdown().expect("close recovered store");

    let (verified, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("verify recovery");
    assert_eq!(
        verified
            .load_tool_artifact("task_orphan", "artifact_execute_1")
            .expect("load recovered artifact")
            .terminal_outputs["terminal_1"],
        "xy"
    );
    verified.shutdown().expect("close verified store");
}

#[test]
fn corrupt_tool_artifact_does_not_make_task_snapshot_unreadable() {
    let root = TempDir::new().expect("create state root");
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(task_projection(
            "task_artifact_damage",
        )))
        .expect("admit create")
        .wait()
        .expect("commit create");
    store
        .submit(TaskWrite::stream_append_terminal(
            "task_artifact_damage",
            "artifact_execute_1",
            "terminal_1",
            "output",
        ))
        .expect("admit output");
    store
        .submit(TaskWrite::barrier("task_artifact_damage"))
        .expect("admit barrier")
        .wait()
        .expect("commit output");
    store.shutdown().expect("close store");

    corrupt_final_byte(&artifact_journal_path(root.path(), "task_artifact_damage"));

    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("reopen state root");
    assert_eq!(
        reopened
            .load("task_artifact_damage")
            .expect("Task remains readable")
            .task
            .task_id,
        "task_artifact_damage"
    );
    assert!(reopened
        .load_tool_artifact("task_artifact_damage", "artifact_execute_1")
        .expect_err("artifact must be unavailable")
        .to_string()
        .contains("checksum mismatch"));
    reopened.shutdown().expect("close reopened store");
}

#[test]
fn empty_terminal_append_is_a_semantic_noop() {
    let root = TempDir::new().expect("create state root");
    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    store
        .submit(TaskWrite::barrier_create(task_projection("task_empty")))
        .expect("admit create")
        .wait()
        .expect("commit create");
    let _ = commits.recv().expect("receive create");

    let receipt = store
        .submit(TaskWrite::stream_append_terminal(
            "task_empty",
            "artifact_execute_1",
            "terminal_1",
            "",
        ))
        .expect("admit empty append");
    let barrier = store
        .submit(TaskWrite::barrier("task_empty"))
        .expect("admit barrier")
        .wait()
        .expect("flush no-op");
    assert_eq!(receipt.wait().expect("resolve no-op").journal_sequence, 1);
    assert_eq!(barrier.journal_sequence, 1);
    assert!(commits.try_recv().is_err());
    assert!(store
        .load_tool_artifact("task_empty", "artifact_execute_1")
        .is_err());
    store.shutdown().expect("close store");
}

#[test]
fn identical_task_replace_and_empty_text_create_no_frame_or_event() {
    let root = TempDir::new().unwrap();
    let mut projection = task_projection("task_noop");
    projection
        .messages
        .push(agent_message("agent_message", "start"));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection.clone()))
        .unwrap()
        .wait()
        .unwrap();
    commits.recv().unwrap();
    let task_dir = root.path().join("task-store-v1/tasks/task_noop");
    let before_task = std::fs::read(task_dir.join("task.json")).unwrap();
    let before_chat = std::fs::read(task_dir.join("chat.snapshot")).unwrap();

    let replace = store
        .submit(TaskWrite::barrier_replace_task(projection.task.clone()))
        .unwrap()
        .wait()
        .unwrap();
    let empty = store
        .submit(TaskWrite::stream_append_text(
            "task_noop",
            "agent_message",
            "",
            "2",
        ))
        .unwrap()
        .wait()
        .unwrap();

    assert_eq!(replace.journal_sequence, 1);
    assert_eq!(empty.journal_sequence, 1);
    assert_eq!(
        std::fs::read(task_dir.join("task.json")).unwrap(),
        before_task
    );
    assert_eq!(
        std::fs::read(task_dir.join("chat.snapshot")).unwrap(),
        before_chat
    );
    assert!(commits.try_recv().is_err());
    store.shutdown().unwrap();
}

#[test]
fn randomized_operations_match_an_independent_model_across_restart_and_compaction() {
    let root = TempDir::new().unwrap();
    let mut projection = task_projection("task_model");
    projection.messages.push(agent_message("agent_message", ""));
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (mut store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    let mut text_model = String::new();
    let mut terminal_model = String::new();
    let mut completed_model = false;
    let mut seed = 0x5eed_u64;

    for index in 0..500 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        match seed % 3 {
            0 => {
                let chunk = char::from(b'a' + (seed % 26) as u8).to_string();
                text_model.push_str(&chunk);
                store
                    .submit(TaskWrite::stream_append_text(
                        "task_model",
                        "agent_message",
                        chunk,
                        index.to_string(),
                    ))
                    .unwrap();
            }
            1 => {
                let chunk = char::from(b'A' + (seed % 26) as u8).to_string();
                terminal_model.push_str(&chunk);
                store
                    .submit(TaskWrite::stream_append_terminal(
                        "task_model",
                        "artifact_execute_1",
                        "terminal_1",
                        chunk,
                    ))
                    .unwrap();
            }
            _ => {
                let mut task = store.load("task_model").unwrap().task;
                completed_model = !completed_model;
                task.status = if completed_model {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Inactive
                };
                store
                    .submit(TaskWrite::barrier_replace_task(task))
                    .unwrap()
                    .wait()
                    .unwrap();
            }
        }
        if index % 50 == 49 {
            store
                .submit(TaskWrite::compaction_barrier("task_model"))
                .unwrap()
                .wait()
                .unwrap();
            assert_model(&store, &text_model, &terminal_model, completed_model);
            store.shutdown().unwrap();
            let (reopened, _events) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
            store = reopened;
            assert_model(&store, &text_model, &terminal_model, completed_model);
        }
    }
    store.shutdown().unwrap();
}

fn assert_model(store: &TaskJournalStore, text: &str, terminal: &str, completed: bool) {
    let projection = store.load("task_model").unwrap();
    assert_eq!(agent_text(&projection.messages[0]), text);
    assert_eq!(projection.task.status == TaskStatus::Completed, completed);
    if !terminal.is_empty() {
        assert_eq!(
            store
                .load_tool_artifact("task_model", "artifact_execute_1")
                .unwrap()
                .terminal_outputs["terminal_1"],
            terminal
        );
    }
}

#[cfg(unix)]
#[test]
fn append_failure_freezes_only_the_affected_task() {
    use std::os::unix::fs::PermissionsExt;

    let root = TempDir::new().expect("create state root");
    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open store");
    let failures = store.subscribe_failures();
    for task_id in ["task_frozen", "task_healthy"] {
        store
            .submit(TaskWrite::barrier_create(task_projection(task_id)))
            .expect("admit create")
            .wait()
            .expect("commit create");
    }
    while commits.try_recv().is_ok() {}

    let task_dir = root.path().join("task-store-v1/tasks/task_frozen");
    std::fs::set_permissions(&task_dir, std::fs::Permissions::from_mode(0o555))
        .expect("make Task storage unwritable");
    let mut frozen = store.load("task_frozen").expect("load Task").task;
    frozen.status = TaskStatus::Completed;
    let error = store
        .submit(TaskWrite::barrier_replace_task(frozen))
        .expect("admit failing write")
        .wait()
        .expect_err("append must fail");
    assert!(error.to_string().contains("Task storage is frozen"));
    assert_eq!(
        failures
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("write failure is reported to runtime safety monitors")
            .task_id,
        "task_frozen"
    );
    assert!(store.load("task_frozen").is_err(), "Task must freeze");
    assert!(commits.try_recv().is_err(), "failed write must not publish");

    let mut healthy = store.load("task_healthy").expect("load healthy Task").task;
    healthy.status = TaskStatus::Completed;
    store
        .submit(TaskWrite::barrier_replace_task(healthy))
        .expect("admit healthy write")
        .wait()
        .expect("unrelated Task remains writable");

    std::fs::set_permissions(&task_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore Task storage permissions");
    store.shutdown().expect("close store");

    let (reopened, _events) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert!(reopened.load("task_frozen").is_err());
    assert_eq!(
        reopened.load("task_healthy").unwrap().task.status,
        TaskStatus::Completed
    );
    reopened.shutdown().unwrap();
}

fn corrupt_final_checksum(root: &std::path::Path, task_id: &str) {
    let path = task_journal_path(root, task_id);
    corrupt_final_byte(&path);
}

fn corrupt_final_byte(path: &std::path::Path) {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("open journal for fault injection");
    file.seek(SeekFrom::End(-1)).expect("seek to checksum");
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte).expect("read checksum byte");
    file.seek(SeekFrom::End(-1)).expect("rewind checksum");
    file.write_all(&[byte[0] ^ 0xff]).expect("damage checksum");
    file.sync_all().expect("sync injected damage");
}

fn artifact_journal_path(root: &std::path::Path, task_id: &str) -> std::path::PathBuf {
    root.join("task-store-v1")
        .join("tasks")
        .join(task_id)
        .join("artifacts")
        .join("artifact_execute_1.journal")
}

fn task_journal_path(root: &std::path::Path, task_id: &str) -> std::path::PathBuf {
    root.join("task-store-v1")
        .join("tasks")
        .join(task_id)
        .join("chat.journal")
}

fn append_incomplete_frame(root: &std::path::Path, task_id: &str) {
    let path = root
        .join("task-store-v1")
        .join("tasks")
        .join(task_id)
        .join("chat.journal");
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open journal for crash injection");
    file.write_all(&64_u64.to_le_bytes())
        .expect("write incomplete frame length");
    file.write_all(b"partial payload")
        .expect("write incomplete payload");
    file.sync_all().expect("sync injected tail");
}

fn append_complete_orphan_artifact_frame(root: &std::path::Path, task_id: &str) {
    let path = root
        .join("task-store-v1")
        .join("tasks")
        .join(task_id)
        .join("artifacts")
        .join("artifact_execute_1.journal");
    let payload = br#"{"format_version":1,"sequence":2,"operations":[{"operation":"append_terminal","terminal_id":"terminal_1","data":"orphan"}]}"#;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open artifact for crash injection");
    file.write_all(&(payload.len() as u64).to_le_bytes())
        .expect("write orphan length");
    file.write_all(payload).expect("write orphan payload");
    file.write_all(&crc32(payload).to_le_bytes())
        .expect("write orphan checksum");
    file.sync_all().expect("sync orphan frame");
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn agent_message(identity: &str, text: &str) -> StoredMessage {
    StoredMessage {
        sequence: 1,
        chat: ChatMessage {
            cursor: "0000000000000001".to_string(),
            identity: identity.to_string(),
            message_type: "agent_message".to_string(),
            message_id: identity.to_string(),
            message: NormalizedMessage::AgentMessage {
                id: identity.to_string(),
                role: AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: text.to_string(),
                }],
                created_at: "2026-07-20T00:00:00Z".to_string(),
            },
        },
    }
}

fn agent_text(message: &StoredMessage) -> &str {
    let NormalizedMessage::AgentMessage { parts, .. } = &message.chat.message else {
        panic!("expected Agent message");
    };
    let [AgentMessagePart::Text { text }] = parts.as_slice() else {
        panic!("expected one text part");
    };
    text
}

fn task_projection(task_id: &str) -> TaskProjection {
    TaskProjection {
        task: TaskRecord {
            task_id: task_id.to_string(),
            title: Default::default(),
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
            lifecycle: TaskLifecycle::Open,
            agent_session_id: Some("session_1".to_string()),
            active_turn_id: None,
            active_turn_started_at: None,
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
