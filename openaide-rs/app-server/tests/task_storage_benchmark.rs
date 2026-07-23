//! Reproducible comparison for the rewrite-heavy incident workload.
//!
//! Run with:
//! `cargo test -p openaide-app-server --release --test task_storage_benchmark -- --ignored --nocapture`

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use openaide_app_server::agent::{
    AgentEventSink, AgentPrompt, AgentPromptOutcome, AgentRuntime, AgentSession, AgentSessionStart,
};
use openaide_app_server::protocol::errors::RuntimeError;
use openaide_app_server::protocol::model::{
    AgentMessagePart, AgentMessageRole, ChatMessage, IsolationKind, NormalizedMessage, TaskStatus,
};
use openaide_app_server::protocol::params::{TaskCreateMode, TaskCreateParams, TaskIdParams};
use openaide_app_server::storage::records::{
    MessageMeta, StoredMessage, TaskConfigMutationState, TaskLifecycle, TaskPreparationRecord,
    TaskRecord,
};
use openaide_app_server::storage::task_journal::{
    CommitReceipt, TaskJournalStore, TaskProjection, TaskWrite,
};
use openaide_app_server::storage::Store;
use openaide_app_server::tasks::TaskService;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::TempDir;

const HISTORY_BYTES: usize = 4 * 1024 * 1024;
const FULL_DELTA_COUNT: usize = 10_002;
const PROFILE_OUTPUT_BYTES: usize = 372_861;
const UNRELATED_TASK_COUNT: usize = 16;
const UNRELATED_HISTORY_BYTES: usize = 256 * 1024;

#[test]
#[ignore = "writes the bounded legacy baseline and full incident workload"]
fn compare_legacy_rewrites_with_task_journal_deltas() {
    let mut legacy = Vec::new();
    for operations in [16, 64, 256] {
        legacy.push(benchmark_legacy(operations));
    }
    let journals = [64 * 1024, 1024 * 1024, HISTORY_BYTES]
        .into_iter()
        .map(|history_bytes| benchmark_journal(history_bytes, FULL_DELTA_COUNT))
        .collect::<Vec<_>>();

    for result in &legacy {
        println!("{}", serde_json::to_string(result).unwrap());
    }
    for result in &journals {
        println!("{}", serde_json::to_string(result).unwrap());
    }

    let largest = legacy.last().unwrap();
    assert!(largest["disk_bytes"].as_u64().unwrap() > HISTORY_BYTES as u64);
    assert!(journals
        .iter()
        .all(|journal| journal["output_bytes"] == PROFILE_OUTPUT_BYTES));
    assert!(journals
        .iter()
        .all(|journal| journal["durable_batches"].as_u64().unwrap() <= 50));
    assert!(journals.iter().all(|journal| {
        journal["operations"] == FULL_DELTA_COUNT
            && journal["publication_events"] == journal["durable_batches"]
            // Each batch durably publishes artifact bytes, its Chat reference,
            // and compact Task metadata; the first artifact adds four anchors.
            && journal["sync_calls"].as_u64().unwrap()
                == journal["durable_batches"].as_u64().unwrap() * 3 + 4
            && journal["peak_task_queued_bytes"].as_u64().unwrap() <= 2 * 1024 * 1024
            && journal["shutdown_publication_events"].as_u64().unwrap() <= 50
    }));
}

fn benchmark_legacy(operations: usize) -> serde_json::Value {
    let root = TempDir::new().expect("create legacy state root");
    let path = root.path().join("messages.json");
    let mut setup_sync_calls = 0;
    write_legacy_projection(
        &path,
        &LegacyProjection {
            history: "x".repeat(HISTORY_BYTES),
            tool_output_preview: String::new(),
        },
        &mut setup_sync_calls,
    );

    let started = Instant::now();
    let mut latencies = Vec::with_capacity(operations);
    let mut physical_frame_bytes = 0_usize;
    let mut logical_update_bytes = 0_usize;
    let mut sync_calls = 0_usize;
    for index in 0..operations {
        let operation_started = Instant::now();
        let bytes = fs::read(&path).expect("read legacy projection");
        let mut projection: LegacyProjection =
            serde_json::from_slice(&bytes).expect("parse legacy projection");
        projection.tool_output_preview = format!("update-{index}");
        logical_update_bytes += projection.tool_output_preview.len();
        physical_frame_bytes += write_legacy_projection(&path, &projection, &mut sync_calls);
        latencies.push(operation_started.elapsed());
    }
    let elapsed = started.elapsed();

    json!({
        "store": "legacy_json_rewrite",
        "history_bytes": HISTORY_BYTES,
        "operations": operations,
        "wall_ms": elapsed.as_secs_f64() * 1000.0,
        "latency_p50_ms": percentile_ms(&mut latencies, 50),
        "latency_p95_ms": percentile_ms(&mut latencies, 95),
        "latency_max_ms": latencies.iter().max().unwrap().as_secs_f64() * 1000.0,
        "disk_bytes": directory_bytes(root.path()),
        "logical_update_bytes": logical_update_bytes,
        "physical_frame_bytes": physical_frame_bytes,
        "sync_calls": sync_calls,
        "publication_events": operations,
    })
}

#[derive(Deserialize, Serialize)]
struct LegacyProjection {
    history: String,
    tool_output_preview: String,
}

/// Models the old read/serialize/atomic-replace path without calling the cut-over Store facade.
fn write_legacy_projection(
    path: &Path,
    projection: &LegacyProjection,
    sync_calls: &mut usize,
) -> usize {
    let bytes = serde_json::to_vec(projection).expect("serialize legacy projection");
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary).expect("create legacy temporary file");
    file.write_all(&bytes).expect("write legacy projection");
    sync_all_counted(&file, sync_calls, "sync legacy projection");
    fs::rename(&temporary, path).expect("publish legacy projection");
    let parent = fs::File::open(path.parent().expect("legacy parent")).expect("open legacy parent");
    sync_all_counted(&parent, sync_calls, "sync legacy parent");
    bytes.len()
}

fn sync_all_counted(file: &fs::File, sync_calls: &mut usize, context: &str) {
    file.sync_all()
        .unwrap_or_else(|error| panic!("{context}: {error}"));
    *sync_calls += 1;
}

fn benchmark_journal(history_bytes: usize, operations: usize) -> serde_json::Value {
    let root = TempDir::new().expect("create journal state root");
    let mut projection = task_projection("task_journal");
    projection.messages.push(StoredMessage {
        sequence: 1,
        chat: agent_chat("history", history_bytes),
    });
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open journal");
    for index in 0..UNRELATED_TASK_COUNT {
        let task_id = format!("task_unrelated_{index}");
        let mut unrelated = task_projection(&task_id);
        unrelated.messages.push(StoredMessage {
            sequence: 1,
            chat: agent_chat(&format!("unrelated-{index}"), UNRELATED_HISTORY_BYTES),
        });
        unrelated.message_meta.message_count = 1;
        unrelated.message_meta.version = 1;
        store
            .submit(TaskWrite::barrier_create(unrelated))
            .expect("admit unrelated Task")
            .wait()
            .expect("commit unrelated Task");
    }
    assert_eq!(commits.try_iter().count(), UNRELATED_TASK_COUNT);
    store
        .submit(TaskWrite::barrier_create(projection))
        .expect("admit create")
        .wait()
        .expect("commit create");
    let _ = commits.recv().expect("receive create commit");
    let disk_bytes_after_create = directory_bytes(root.path());
    let sync_calls_before_workload = store.durability_sync_calls();

    let started = Instant::now();
    let mut admission_latencies = Vec::with_capacity(operations);
    let (receipt_sender, receipt_receiver) = std::sync::mpsc::channel::<(Instant, CommitReceipt)>();
    let receipt_collector = thread::spawn(move || {
        let mut durable_latencies = Vec::with_capacity(operations);
        for (operation_started, receipt) in receipt_receiver {
            receipt.wait().expect("commit terminal delta");
            durable_latencies.push(operation_started.elapsed());
        }
        durable_latencies
    });
    for index in 0..operations {
        let operation_started = Instant::now();
        let chunk_bytes = PROFILE_OUTPUT_BYTES / operations
            + usize::from(index < PROFILE_OUTPUT_BYTES % operations);
        let receipt = store
            .submit(TaskWrite::stream_append_terminal(
                "task_journal",
                "artifact_execute_1",
                "terminal_1",
                "x".repeat(chunk_bytes),
            ))
            .expect("admit terminal delta");
        admission_latencies.push(operation_started.elapsed());
        receipt_sender
            .send((operation_started, receipt))
            .expect("collect durable receipt");
    }
    drop(receipt_sender);
    let barrier_started = Instant::now();
    store
        .submit(TaskWrite::barrier("task_journal"))
        .expect("admit barrier")
        .wait()
        .expect("flush terminal deltas");
    let barrier_latency = barrier_started.elapsed();
    let elapsed = started.elapsed();
    let mut durable_latencies = receipt_collector.join().expect("join receipt collector");
    let sync_calls = store
        .durability_sync_calls()
        .saturating_sub(sync_calls_before_workload);
    let durable_commits = commits.try_iter().collect::<Vec<_>>();
    let durable_batches = durable_commits.len();
    let publication_events = durable_commits.len();
    let published_terminal_append_events = durable_commits
        .iter()
        .flat_map(|commit| &commit.artifact_changes)
        .map(|change| change.terminal_appends.len())
        .sum::<usize>();
    let queue_metrics = store.queue_metrics();
    let output_bytes = store
        .load_tool_artifact("task_journal", "artifact_execute_1")
        .expect("load output")
        .terminal_outputs["terminal_1"]
        .len();
    let disk_bytes_before_compaction = directory_bytes(root.path());
    let physical_frame_bytes = disk_bytes_before_compaction.saturating_sub(disk_bytes_after_create);
    let compaction_started = Instant::now();
    store
        .submit(TaskWrite::compaction_barrier("task_journal"))
        .expect("admit compaction barrier")
        .wait()
        .expect("compact Task journal");
    let compaction_ms = compaction_started.elapsed().as_secs_f64() * 1000.0;
    let disk_bytes_after_compaction = directory_bytes(root.path());
    store.shutdown().expect("close journal");

    let startup_started = Instant::now();
    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("replay journal");
    let startup_ms = startup_started.elapsed().as_secs_f64() * 1000.0;
    let task_load_started = Instant::now();
    reopened.load("task_journal").expect("lazy-load Task");
    let task_load_ms = task_load_started.elapsed().as_secs_f64() * 1000.0;
    let artifact_load_started = Instant::now();
    let replayed_output = reopened
        .load_tool_artifact("task_journal", "artifact_execute_1")
        .expect("lazy-load output");
    let artifact_load_ms = artifact_load_started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(
        replayed_output.terminal_outputs["terminal_1"].len(),
        PROFILE_OUTPUT_BYTES
    );
    reopened.shutdown().expect("close replayed journal");
    let shutdown_drain = benchmark_shutdown_drain(history_bytes, operations);
    let product_stop = benchmark_product_stop();

    json!({
        "store": "task_journal",
        "history_bytes": history_bytes,
        "unrelated_tasks": UNRELATED_TASK_COUNT,
        "unrelated_history_bytes_each": UNRELATED_HISTORY_BYTES,
        "operations": operations,
        "wall_ms": elapsed.as_secs_f64() * 1000.0,
        "admission_p50_ms": percentile_ms(&mut admission_latencies, 50),
        "admission_p95_ms": percentile_ms(&mut admission_latencies, 95),
        "admission_max_ms": admission_latencies.iter().max().unwrap().as_secs_f64() * 1000.0,
        "durable_p50_ms": percentile_ms(&mut durable_latencies, 50),
        "durable_p95_ms": percentile_ms(&mut durable_latencies, 95),
        "durable_max_ms": durable_latencies.iter().max().unwrap().as_secs_f64() * 1000.0,
        "barrier_ms": barrier_latency.as_secs_f64() * 1000.0,
        "durable_batches": durable_batches,
        "publication_events": publication_events,
        "published_terminal_append_events": published_terminal_append_events,
        "output_bytes": output_bytes,
        "logical_delta_bytes": PROFILE_OUTPUT_BYTES,
        "physical_frame_bytes": physical_frame_bytes,
        "sync_calls": sync_calls,
        "peak_global_queued_bytes": queue_metrics.peak_global_stream_bytes,
        "peak_task_queued_bytes": queue_metrics.peak_task_stream_bytes,
        "shutdown_drain_ms": shutdown_drain.latency.as_secs_f64() * 1000.0,
        "shutdown_publication_events": shutdown_drain.publication_events,
        "product_stop_ms": product_stop.as_secs_f64() * 1000.0,
        "disk_bytes_before_compaction": disk_bytes_before_compaction,
        "disk_bytes_after_compaction": disk_bytes_after_compaction,
        "compaction_ms": compaction_ms,
        "startup_ms": startup_ms,
        "task_load_ms": task_load_ms,
        "artifact_load_ms": artifact_load_ms,
        "task_revision": 1,
    })
}

struct ShutdownDrainResult {
    latency: Duration,
    publication_events: usize,
}

/// Measures process shutdown against the same complete flood as the explicit barrier run.
/// Shutdown must drain admitted work before joining, so restart is the oracle.
fn benchmark_shutdown_drain(history_bytes: usize, operations: usize) -> ShutdownDrainResult {
    let root = TempDir::new().expect("create stop benchmark state root");
    let mut projection = task_projection("task_stop");
    projection.messages.push(StoredMessage {
        sequence: 1,
        chat: agent_chat("history", history_bytes),
    });
    projection.message_meta.message_count = 1;
    projection.message_meta.version = 1;
    let (store, commits) = TaskJournalStore::open(root.path().to_path_buf()).expect("open journal");
    store
        .submit(TaskWrite::barrier_create(projection))
        .expect("admit stop fixture")
        .wait()
        .expect("commit stop fixture");
    let _ = commits.recv().expect("receive stop fixture commit");
    submit_terminal_workload(&store, "task_stop", operations);

    let stop_started = Instant::now();
    store.shutdown().expect("Stop drains journal");
    let latency = stop_started.elapsed();
    let publication_events = commits.try_iter().count();

    let (reopened, _commits) =
        TaskJournalStore::open(root.path().to_path_buf()).expect("replay stopped journal");
    assert_eq!(
        reopened
            .load_tool_artifact("task_stop", "artifact_execute_1")
            .expect("replay stopped output")
            .terminal_outputs["terminal_1"]
            .len(),
        PROFILE_OUTPUT_BYTES
    );
    reopened.shutdown().expect("close stopped replay");
    ShutdownDrainResult {
        latency,
        publication_events,
    }
}

struct BenchmarkStopAgent {
    started: AtomicBool,
}

impl AgentRuntime for BenchmarkStopAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "benchmark_stop_session",
        ))
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<AgentPromptOutcome, RuntimeError> {
        self.started.store(true, Ordering::Release);
        while !prompt.cancellation.is_cancelled() {
            thread::yield_now();
        }
        Ok(AgentPromptOutcome::Cancelled)
    }
}

/// Measures the production Stop workflow's durable `stopping` transition and
/// Agent cancellation dispatch. Protocol publication is verified separately.
fn benchmark_product_stop() -> Duration {
    let root = TempDir::new().expect("create product Stop benchmark root");
    let agent = Arc::new(BenchmarkStopAgent {
        started: AtomicBool::new(false),
    });
    let store = Store::open(root.path().join("state")).expect("open product Stop store");
    let service = TaskService::new(store, agent.clone());
    let snapshot = service
        .create(TaskCreateParams {
            mode: TaskCreateMode::PromptStart,
            title: "Stop benchmark".to_string(),
            workspace_root: root.path().to_string_lossy().to_string(),
            selected_agent_id: "codex".to_string(),
            selected_agent_label: None,
            selected_isolation: IsolationKind::Local,
            prompt_text: Some("wait for cancellation".to_string()),
            external_session_id: None,
            model_id: None,
            context: Vec::new(),
        })
        .expect("start product Stop fixture");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !agent.started.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::yield_now();
    }
    assert!(
        agent.started.load(Ordering::Acquire),
        "Agent prompt did not start"
    );

    let started = Instant::now();
    let stopped = service
        .cancel(TaskIdParams {
            task_id: snapshot.task.task_id,
        })
        .expect("production Stop succeeds");
    let latency = started.elapsed();
    assert!(matches!(
        stopped.task.status,
        TaskStatus::Stopping | TaskStatus::Inactive
    ));
    service.shutdown().expect("close product Stop fixture");
    latency
}

fn submit_terminal_workload(store: &TaskJournalStore, task_id: &str, operations: usize) {
    for index in 0..operations {
        let chunk_bytes = PROFILE_OUTPUT_BYTES / operations
            + usize::from(index < PROFILE_OUTPUT_BYTES % operations);
        store
            .submit(TaskWrite::stream_append_terminal(
                task_id,
                "artifact_execute_1",
                "terminal_1",
                "x".repeat(chunk_bytes),
            ))
            .expect("admit terminal delta");
    }
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
    }
}
