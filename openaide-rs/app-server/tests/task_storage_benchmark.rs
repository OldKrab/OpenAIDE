//! Reproducible comparison for the rewrite-heavy incident workload.
//!
//! Run with:
//! `cargo test -p openaide-app-server --release --test task_storage_benchmark -- --ignored --nocapture`

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use openaide_app_server::protocol::model::{
    AgentMessagePart, AgentMessageRole, ChatMessage, IsolationKind, NormalizedMessage, TaskStatus,
};
use openaide_app_server::storage::records::{
    MessageMeta, StoredMessage, TaskConfigMutationState, TaskLifecycle, TaskPreparationRecord,
    TaskRecord,
};
use openaide_app_server::storage::task_journal::{TaskJournalStore, TaskProjection, TaskWrite};
use openaide_app_server::storage::Store;
use serde_json::json;
use tempfile::TempDir;

const HISTORY_BYTES: usize = 4 * 1024 * 1024;
const FULL_DELTA_COUNT: usize = 10_000;
const PROFILE_OUTPUT_BYTES: usize = 372_861;

#[test]
#[ignore = "writes the bounded legacy baseline and full incident workload"]
fn compare_legacy_rewrites_with_task_journal_deltas() {
    let mut legacy = Vec::new();
    for operations in [16, 64, 256] {
        legacy.push(benchmark_legacy(operations));
    }
    let journal = benchmark_journal(FULL_DELTA_COUNT);

    for result in &legacy {
        println!("{}", serde_json::to_string(result).unwrap());
    }
    println!("{}", serde_json::to_string(&journal).unwrap());

    let largest = legacy.last().unwrap();
    assert!(largest["disk_bytes"].as_u64().unwrap() > HISTORY_BYTES as u64);
    assert_eq!(journal["output_bytes"], PROFILE_OUTPUT_BYTES);
    assert!(journal["durable_batches"].as_u64().unwrap() <= 50);
}

fn benchmark_legacy(operations: usize) -> serde_json::Value {
    let root = TempDir::new().expect("create legacy state root");
    let store = Store::open(root.path().to_path_buf()).expect("open legacy store");
    let task = task_record("task_legacy");
    store.write_task(&task).expect("write legacy Task");
    store
        .append_message("task_legacy", agent_chat("history", HISTORY_BYTES))
        .expect("write large history");
    let update = agent_chat("tool_update", 1);

    let started = Instant::now();
    let mut latencies = Vec::with_capacity(operations);
    for _ in 0..operations {
        let operation_started = Instant::now();
        store
            .upsert_message_by_identity("task_legacy", update.clone())
            .expect("rewrite legacy history");
        latencies.push(operation_started.elapsed());
    }
    let elapsed = started.elapsed();
    store.mark_clean_shutdown().expect("mark legacy shutdown");

    json!({
        "store": "legacy_json_rewrite",
        "history_bytes": HISTORY_BYTES,
        "operations": operations,
        "wall_ms": elapsed.as_secs_f64() * 1000.0,
        "latency_p50_ms": percentile_ms(&mut latencies, 50),
        "latency_p95_ms": percentile_ms(&mut latencies, 95),
        "latency_max_ms": latencies.iter().max().unwrap().as_secs_f64() * 1000.0,
        "disk_bytes": directory_bytes(root.path()),
        "estimated_serialized_bytes": HISTORY_BYTES.saturating_mul(operations),
    })
}

fn benchmark_journal(operations: usize) -> serde_json::Value {
    let root = TempDir::new().expect("create journal state root");
    let mut projection = task_projection("task_journal");
    projection.messages.push(StoredMessage {
        sequence: 1,
        chat: agent_chat("history", HISTORY_BYTES),
    });
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open journal");
    store
        .submit(TaskWrite::barrier_create(projection))
        .expect("admit create")
        .wait()
        .expect("commit create");
    let _ = commits.recv().expect("receive create commit");

    let started = Instant::now();
    let mut latencies = Vec::with_capacity(operations);
    for index in 0..operations {
        let operation_started = Instant::now();
        let chunk_bytes = PROFILE_OUTPUT_BYTES / operations
            + usize::from(index < PROFILE_OUTPUT_BYTES % operations);
        store
            .submit(TaskWrite::stream_append_terminal(
                "task_journal",
                "artifact_execute_1",
                "terminal_1",
                "x".repeat(chunk_bytes),
            ))
            .expect("admit terminal delta");
        latencies.push(operation_started.elapsed());
    }
    let barrier_started = Instant::now();
    store
        .submit(TaskWrite::barrier("task_journal"))
        .expect("admit barrier")
        .wait()
        .expect("flush terminal deltas");
    let barrier_latency = barrier_started.elapsed();
    let elapsed = started.elapsed();
    let durable_batches = commits.try_iter().count();
    let output_bytes = store
        .load_tool_artifact("task_journal", "artifact_execute_1")
        .expect("load output")
        .terminal_outputs["terminal_1"]
        .len();
    store.shutdown().expect("close journal");

    let replay_started = Instant::now();
    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("replay journal");
    let replay_ms = replay_started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(
        reopened
            .load_tool_artifact("task_journal", "artifact_execute_1")
            .expect("replay output")
            .terminal_outputs["terminal_1"]
            .len(),
        PROFILE_OUTPUT_BYTES
    );
    reopened.shutdown().expect("close replayed journal");

    json!({
        "store": "task_journal",
        "history_bytes": HISTORY_BYTES,
        "operations": operations,
        "wall_ms": elapsed.as_secs_f64() * 1000.0,
        "admission_p50_ms": percentile_ms(&mut latencies, 50),
        "admission_p95_ms": percentile_ms(&mut latencies, 95),
        "admission_max_ms": latencies.iter().max().unwrap().as_secs_f64() * 1000.0,
        "barrier_ms": barrier_latency.as_secs_f64() * 1000.0,
        "durable_batches": durable_batches,
        "output_bytes": output_bytes,
        "disk_bytes": directory_bytes(root.path()),
        "replay_ms": replay_ms,
        "task_revision": 1,
    })
}

fn percentile_ms(values: &mut [Duration], percentile: usize) -> f64 {
    values.sort_unstable();
    let index = (values.len() - 1) * percentile / 100;
    values[index].as_secs_f64() * 1000.0
}

fn directory_bytes(path: &Path) -> u64 {
    fs::read_dir(path)
        .expect("read benchmark directory")
        .map(|entry| {
            let entry = entry.expect("read benchmark entry");
            if entry.file_type().expect("read entry type").is_dir() {
                directory_bytes(&entry.path())
            } else {
                entry.metadata().expect("read entry metadata").len()
            }
        })
        .sum()
}

fn agent_chat(identity: &str, text_bytes: usize) -> ChatMessage {
    ChatMessage {
        cursor: String::new(),
        identity: identity.to_string(),
        message_type: "agent_message".to_string(),
        message_id: identity.to_string(),
        message: NormalizedMessage::AgentMessage {
            id: identity.to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![AgentMessagePart::Text {
                text: "x".repeat(text_bytes),
            }],
            created_at: "2026-07-20T00:00:00Z".to_string(),
        },
    }
}

fn task_projection(task_id: &str) -> TaskProjection {
    TaskProjection {
        task: task_record(task_id),
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

fn task_record(task_id: &str) -> TaskRecord {
    TaskRecord {
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
        config_options: HashMap::new(),
        config_options_catalog: None,
        config_mutation: TaskConfigMutationState::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
