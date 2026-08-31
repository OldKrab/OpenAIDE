use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use openaide_app_server_protocol::diagnostics::{
    SupportArtifactAvailability, SupportExportCreateParams, SupportExportListParams,
    SupportExportListResult, SupportExportSession, SupportExportTrace,
};
use openaide_app_server_protocol::ids::{AgentId, TaskId};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus;
use crate::storage::records::TaskRecord;
use crate::storage::Store;

mod safe_logs;
mod zip;

use self::safe_logs::safe_log_snapshot;
use self::zip::write_stored_zip;

const MAX_ACP_TRACE_EXPORT_BYTES: u64 = 2 * 1024 * 1024;
const SUPPORT_EXPORT_RETENTION_SECS: u64 = 60 * 60;

pub(crate) struct CreatedSupportExport {
    pub(crate) path: PathBuf,
    pub(crate) label: String,
    pub(crate) size_bytes: u64,
    pub(crate) contains_sensitive_data: bool,
}

#[derive(Clone)]
struct TraceArtifact {
    path: PathBuf,
    id: String,
    task_id: Option<String>,
    session_id: Option<String>,
    operation: String,
    modified_at: String,
    size_bytes: u64,
}

pub(crate) fn list(
    store: &Store,
    params: SupportExportListParams,
) -> Result<SupportExportListResult, RuntimeError> {
    let traces = trace_artifacts(store.root())?;
    let mut tasks = store.list_all_task_records()?;
    tasks.sort_by(|left, right| right.last_activity.cmp(&left.last_activity));
    if let Some(task_id) = params.task_id.as_ref() {
        tasks.sort_by_key(|task| task.task_id != task_id.as_str());
    }
    let native_transcripts = codex_transcript_index(tasks.iter());
    let sessions = tasks
        .into_iter()
        .filter(|task| task.agent_session_id.is_some())
        .map(|task| session_candidate(&task, &traces, &native_transcripts))
        .collect();
    let bound_sessions = store
        .list_all_task_records()?
        .into_iter()
        .filter_map(|task| task.agent_session_id)
        .collect::<HashSet<_>>();
    let mut unbound_traces = traces
        .into_iter()
        .filter(|trace| {
            trace
                .session_id
                .as_ref()
                .is_none_or(|id| !bound_sessions.contains(id))
        })
        .map(protocol_trace)
        .collect::<Vec<_>>();
    unbound_traces.sort_by(|left, right| right.modified_at.cmp(&left.modified_at));
    Ok(SupportExportListResult {
        sessions,
        unbound_traces,
        acp_trace_enabled: acp_trace_enabled(store),
    })
}

pub(crate) fn create(
    store: &Store,
    params: &SupportExportCreateParams,
) -> Result<CreatedSupportExport, RuntimeError> {
    if !params.include_runtime_snapshot
        && !params.include_logs
        && params.sessions.is_empty()
        && params.unbound_trace_ids.is_empty()
    {
        return Err(RuntimeError::InvalidParams(
            "support export selection is empty".to_string(),
        ));
    }
    let traces = trace_artifacts(store.root())?;
    let task_records = store
        .list_all_task_records()?
        .into_iter()
        .map(|record| (record.task_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let native_transcripts = codex_transcript_index(task_records.values());
    let mut entries = BTreeMap::<String, Vec<u8>>::new();
    let mut source_manifest = Vec::<Value>::new();
    if params.include_runtime_snapshot {
        let records = store.list_all_task_records()?;
        entries.insert("diagnostics/runtime.json".to_string(), json_bytes(&json!({
            "taskCount": records.len(),
            "activeTaskCount": records.iter().filter(|task| task.active_turn_id.is_some()).count(),
            "storageRecovery": format!("{:?}", store.recovery_classification()),
        }))?);
        source_manifest.push(manifest_source("runtime", "included", None));
    }
    if params.include_logs {
        for name in ["openaide-extension.jsonl", "openaide-app-server.jsonl"] {
            let path = store.root().join("diagnostics/logs").join(name);
            match safe_log_snapshot(&path) {
                Ok(bytes) => {
                    entries.insert(format!("logs/{name}"), bytes);
                    source_manifest.push(manifest_source(name, "included", None));
                }
                Err(_) => source_manifest.push(manifest_source(name, "unavailable", None)),
            }
        }
    }
    for selection in &params.sessions {
        let task_id = selection.task_id.as_str();
        let safe_task = safe_segment(task_id);
        let Some(task) = task_records.get(task_id) else {
            add_unavailable_session_sources(&mut source_manifest, selection, task_id);
            continue;
        };
        let session_id = task.agent_session_id.clone();
        let active = task_is_active(task);
        if selection.include_openaide_history {
            match store.task_journal().load(task_id) {
                Ok(projection) => {
                    let chat = projection
                        .messages
                        .iter()
                        .map(|message| {
                            crate::snapshots::task_snapshot::project_chat_item(&message.chat)
                        })
                        .collect::<Vec<_>>();
                    entries.insert(
                        format!("sessions/{safe_task}/openaide-history.json"),
                        json_bytes(&json!({
                            "task": projection.task.summary(),
                            "messages": chat,
                            "messageMeta": projection.message_meta,
                        }))?,
                    );
                    source_manifest.push(manifest_snapshot_source(
                        &format!("task:{task_id}:history"),
                        "included",
                        None,
                        active,
                        "validated-task-projection",
                    ));
                }
                Err(_) => source_manifest.push(manifest_snapshot_source(
                    &format!("task:{task_id}:history"),
                    "unavailable",
                    None,
                    active,
                    "validated-task-projection",
                )),
            }
        }
        if selection.include_acp_traces {
            let mut matched_trace = false;
            for trace in traces.iter().filter(|trace| trace.session_id == session_id) {
                matched_trace = true;
                add_complete_jsonl(
                    &mut entries,
                    &mut source_manifest,
                    trace,
                    &format!("sessions/{safe_task}/acp-traces"),
                    active,
                );
            }
            if !matched_trace {
                source_manifest.push(manifest_snapshot_source(
                    &format!("task:{task_id}:acp-traces"),
                    "unavailable",
                    None,
                    active,
                    "complete-jsonl-prefix",
                ));
            }
        }
        if selection.include_native_transcript {
            match session_id
                .as_deref()
                .and_then(|id| native_transcripts.get(id).cloned())
            {
                Some(path) => add_native_transcript(
                    &mut entries,
                    &mut source_manifest,
                    &path,
                    task_id,
                    &safe_task,
                    active,
                ),
                None => source_manifest.push(manifest_snapshot_source(
                    &format!("task:{task_id}:native"),
                    "unavailable",
                    None,
                    active,
                    "complete-jsonl-prefix",
                )),
            }
        }
    }
    for trace_id in &params.unbound_trace_ids {
        if let Some(trace) = traces.iter().find(|trace| &trace.id == trace_id) {
            add_complete_jsonl(
                &mut entries,
                &mut source_manifest,
                trace,
                "unbound-acp-traces",
                false,
            );
        } else {
            source_manifest.push(manifest_source(
                &format!("trace:{trace_id}"),
                "unavailable",
                None,
            ));
        }
    }
    let contains_sensitive_data = params.sessions.iter().any(|selection| {
        selection.include_openaide_history
            || selection.include_acp_traces
            || selection.include_native_transcript
    }) || !params.unbound_trace_ids.is_empty();
    let created_at = now_millis();
    let label = format!("openaide-support-{created_at}.zip");
    entries.insert("manifest.json".to_string(), json_bytes(&json!({
        "schemaVersion": 2,
        "createdAt": created_at.to_string(),
        "containsSensitiveData": contains_sensitive_data,
        "warning": contains_sensitive_data.then_some("Raw selected artifacts may contain prompts, responses, paths, tool output, and secrets."),
        "sources": source_manifest,
    }))?);
    let export_dir = store.root().join("diagnostics/support-exports");
    fs::create_dir_all(&export_dir)?;
    prune_support_exports(&export_dir);
    let path = export_dir.join(format!("{}-{label}", Uuid::new_v4()));
    write_stored_zip(&path, entries)?;
    let size_bytes = fs::metadata(&path)?.len();
    Ok(CreatedSupportExport {
        path,
        label,
        size_bytes,
        contains_sensitive_data,
    })
}

fn prune_support_exports(directory: &Path) {
    let cutoff = SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(
            SUPPORT_EXPORT_RETENTION_SECS,
        ))
        .unwrap_or(UNIX_EPOCH);
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .is_some_and(|modified| modified < cutoff);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn session_candidate(
    task: &TaskRecord,
    traces: &[TraceArtifact],
    native_transcripts: &BTreeMap<String, PathBuf>,
) -> SupportExportSession {
    let session_id = task.agent_session_id.as_deref();
    SupportExportSession {
        task_id: TaskId::from(task.task_id.clone()),
        title: task
            .title
            .effective()
            .map(|title| title.value().to_string())
            .unwrap_or_else(|| "Untitled Task".to_string()),
        agent_id: AgentId::from(task.agent_id.clone()),
        agent_name: task.agent_name.clone(),
        project_label: Path::new(task.project_root.as_deref().unwrap_or(&task.workspace_root))
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Project")
            .to_string(),
        last_activity: task.last_activity.clone(),
        active: task_is_active(task),
        acp_trace_count: traces
            .iter()
            .filter(|trace| trace.session_id.as_deref() == session_id)
            .count(),
        native_transcript: session_id
            .filter(|id| native_transcripts.contains_key(*id))
            .map(|_| SupportArtifactAvailability::Available)
            .unwrap_or(SupportArtifactAvailability::Unavailable),
    }
}

fn task_is_active(task: &TaskRecord) -> bool {
    matches!(
        task.status,
        TaskStatus::Starting | TaskStatus::Active | TaskStatus::Stopping | TaskStatus::Waiting
    )
}

fn add_unavailable_session_sources(
    manifest: &mut Vec<Value>,
    selection: &openaide_app_server_protocol::diagnostics::SupportExportSessionSelection,
    task_id: &str,
) {
    for (requested, kind) in [
        (selection.include_openaide_history, "history"),
        (selection.include_acp_traces, "acp-traces"),
        (selection.include_native_transcript, "native"),
    ] {
        if requested {
            manifest.push(manifest_source(
                &format!("task:{task_id}:{kind}"),
                "unavailable",
                None,
            ));
        }
    }
}

fn trace_artifacts(root: &Path) -> Result<Vec<TraceArtifact>, RuntimeError> {
    let trace_root = std::env::var_os("OPENAIDE_ACP_TRACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("diagnostics/acp-traces"));
    let Ok(read_dir) = fs::read_dir(trace_root) else {
        return Ok(Vec::new());
    };
    let mut traces = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let (task_id, session_id, operation) = trace_identity(&path);
        traces.push(TraceArtifact {
            id: entry.file_name().to_string_lossy().to_string(),
            path,
            task_id,
            session_id,
            operation,
            modified_at: system_time_millis(metadata.modified().unwrap_or(UNIX_EPOCH)),
            size_bytes: metadata.len(),
        });
    }
    Ok(traces)
}

fn trace_identity(path: &Path) -> (Option<String>, Option<String>, String) {
    let Ok(file) = fs::File::open(path) else {
        return (None, None, "unknown".to_string());
    };
    let mut task_id = None;
    let mut session_id = None;
    let mut operation = "unknown".to_string();
    for line in std::io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .take(200)
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("event").and_then(Value::as_str) == Some("trace_opened") {
            task_id = value
                .pointer("/payload/task_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            operation = value
                .pointer("/payload/operation")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
        session_id = session_id.or_else(|| find_session_id(&value));
        if let Some(raw) = value.pointer("/payload/line").and_then(Value::as_str) {
            if let Ok(raw_value) = serde_json::from_str::<Value>(raw) {
                session_id = session_id.or_else(|| find_session_id(&raw_value));
            }
        }
        if task_id.is_some() && session_id.is_some() {
            break;
        }
    }
    (task_id, session_id, operation)
}

fn find_session_id(value: &Value) -> Option<String> {
    match value {
        Value::Object(object) => object.iter().find_map(|(key, value)| {
            ((key == "sessionId" || key == "session_id") && value.as_str().is_some())
                .then(|| value.as_str().map(str::to_string))
                .flatten()
                .or_else(|| find_session_id(value))
        }),
        Value::Array(values) => values.iter().find_map(find_session_id),
        _ => None,
    }
}

fn protocol_trace(trace: TraceArtifact) -> SupportExportTrace {
    SupportExportTrace {
        trace_id: trace.id,
        task_id: trace.task_id.map(TaskId::from),
        operation: trace.operation,
        modified_at: trace.modified_at,
        size_bytes: trace.size_bytes,
    }
}

/// Codex is the first provider adapter. Every Agent still exposes OpenAIDE history and ACP traces.
fn codex_transcript_index<'a>(
    tasks: impl Iterator<Item = &'a TaskRecord>,
) -> BTreeMap<String, PathBuf> {
    let mut wanted = tasks
        .filter(|task| task.agent_id == "codex")
        .filter_map(|task| task.agent_session_id.clone())
        .collect::<HashSet<_>>();
    if wanted.is_empty() {
        return BTreeMap::new();
    }
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")));
    let Some(home) = home else {
        return BTreeMap::new();
    };
    let mut found = BTreeMap::new();
    for root in [home.join("sessions"), home.join("archived_sessions")] {
        find_files_with_session_ids(&root, &mut wanted, &mut found);
        if wanted.is_empty() {
            break;
        }
    }
    found
}

fn find_files_with_session_ids(
    root: &Path,
    wanted: &mut HashSet<String>,
    found: &mut BTreeMap<String, PathBuf>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if wanted.is_empty() {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            find_files_with_session_ids(&path, wanted, found);
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(session_id) = wanted.iter().find(|id| name.contains(id.as_str())).cloned() {
            wanted.remove(&session_id);
            found.insert(session_id, path);
        }
    }
}

fn add_complete_jsonl(
    entries: &mut BTreeMap<String, Vec<u8>>,
    manifest: &mut Vec<Value>,
    trace: &TraceArtifact,
    directory: &str,
    active: bool,
) {
    match complete_jsonl_tail(&trace.path, MAX_ACP_TRACE_EXPORT_BYTES) {
        Ok((bytes, truncated)) => {
            let exported_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            entries.insert(format!("{directory}/{}", safe_segment(&trace.id)), bytes);
            manifest.push(manifest_bounded_snapshot_source(
                &format!("trace:{}", trace.id),
                if truncated { "truncated" } else { "included" },
                Some(trace.size_bytes),
                Some(exported_size),
                active,
                "latest-complete-jsonl-tail",
            ));
        }
        Err(_) => manifest.push(manifest_snapshot_source(
            &format!("trace:{}", trace.id),
            "unavailable",
            Some(trace.size_bytes),
            active,
            "latest-complete-jsonl-tail",
        )),
    }
}

fn add_native_transcript(
    entries: &mut BTreeMap<String, Vec<u8>>,
    manifest: &mut Vec<Value>,
    path: &Path,
    task_id: &str,
    safe_task: &str,
    active: bool,
) {
    if path.extension().and_then(|value| value.to_str()) == Some("zst") {
        match fs::File::open(path).and_then(zstd::stream::decode_all) {
            Ok(mut bytes) => {
                let complete_len = bytes
                    .iter()
                    .rposition(|byte| *byte == b'\n')
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let truncated = complete_len < bytes.len();
                bytes.truncate(complete_len);
                entries.insert(
                    format!("sessions/{safe_task}/native-transcript.jsonl"),
                    bytes,
                );
                manifest.push(manifest_snapshot_source(
                    &format!("task:{task_id}:native"),
                    if truncated { "truncated" } else { "included" },
                    None,
                    active,
                    "decompressed-complete-jsonl-prefix",
                ));
            }
            Err(_) => manifest.push(manifest_snapshot_source(
                &format!("task:{task_id}:native"),
                "unavailable",
                None,
                active,
                "decompressed-complete-jsonl-prefix",
            )),
        }
        return;
    }
    match complete_prefix(path) {
        Ok((bytes, truncated)) => {
            entries.insert(
                format!("sessions/{safe_task}/native-transcript.jsonl"),
                bytes,
            );
            manifest.push(manifest_snapshot_source(
                &format!("task:{task_id}:native"),
                if truncated { "truncated" } else { "included" },
                None,
                active,
                "complete-jsonl-prefix",
            ));
        }
        Err(_) => manifest.push(manifest_snapshot_source(
            &format!("task:{task_id}:native"),
            "unavailable",
            None,
            active,
            "complete-jsonl-prefix",
        )),
    }
}

fn complete_prefix(path: &Path) -> std::io::Result<(Vec<u8>, bool)> {
    let initial_len = fs::metadata(path)?.len();
    let file = fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(usize::try_from(initial_len).unwrap_or(0));
    file.take(initial_len).read_to_end(&mut bytes)?;
    let complete_len = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let truncated = complete_len < bytes.len()
        || fs::metadata(path)
            .map(|metadata| metadata.len() > initial_len)
            .unwrap_or(false);
    bytes.truncate(complete_len);
    Ok((bytes, truncated))
}

/// Captures a bounded tail without emitting a partial JSONL record at either boundary.
fn complete_jsonl_tail(path: &Path, max_bytes: u64) -> std::io::Result<(Vec<u8>, bool)> {
    let initial_len = fs::metadata(path)?.len();
    let mut file = fs::File::open(path)?;
    let complete_end = last_complete_jsonl_offset(&mut file, initial_len)?;
    let wanted_start = complete_end.saturating_sub(max_bytes);
    let read_start = wanted_start.saturating_sub(1);
    file.seek(SeekFrom::Start(read_start))?;
    let mut bytes =
        Vec::with_capacity(usize::try_from(complete_end.saturating_sub(read_start)).unwrap_or(0));
    file.take(complete_end.saturating_sub(read_start))
        .read_to_end(&mut bytes)?;

    if wanted_start > 0 {
        let first_complete = if bytes.first() == Some(&b'\n') {
            1
        } else {
            bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|index| index + 1)
                .unwrap_or(bytes.len())
        };
        bytes.drain(..first_complete);
    }
    let truncated = wanted_start > 0
        || complete_end < initial_len
        || fs::metadata(path)
            .map(|metadata| metadata.len() > initial_len)
            .unwrap_or(false);
    Ok((bytes, truncated))
}

fn last_complete_jsonl_offset(file: &mut fs::File, file_len: u64) -> std::io::Result<u64> {
    const SCAN_CHUNK_BYTES: usize = 8 * 1024;
    let mut cursor = file_len;
    let mut buffer = vec![0_u8; SCAN_CHUNK_BYTES];
    while cursor > 0 {
        let chunk_len = usize::try_from(cursor.min(SCAN_CHUNK_BYTES as u64)).unwrap_or(0);
        cursor -= chunk_len as u64;
        file.seek(SeekFrom::Start(cursor))?;
        file.read_exact(&mut buffer[..chunk_len])?;
        if let Some(index) = buffer[..chunk_len].iter().rposition(|byte| *byte == b'\n') {
            return Ok(cursor + index as u64 + 1);
        }
    }
    Ok(0)
}

fn acp_trace_enabled(store: &Store) -> bool {
    std::env::var("OPENAIDE_ACP_TRACE")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on" | "full" | "raw"
            )
        })
        || store.read_acp_trace_enabled().unwrap_or(false)
}

fn manifest_source(id: &str, status: &str, original_size: Option<u64>) -> Value {
    json!({ "id": id, "status": status, "originalSizeBytes": original_size })
}

fn manifest_snapshot_source(
    id: &str,
    status: &str,
    original_size: Option<u64>,
    active: bool,
    snapshot_boundary: &str,
) -> Value {
    json!({
        "id": id,
        "status": status,
        "originalSizeBytes": original_size,
        "activeSnapshot": active,
        "snapshotBoundary": snapshot_boundary,
    })
}

fn manifest_bounded_snapshot_source(
    id: &str,
    status: &str,
    original_size: Option<u64>,
    exported_size: Option<u64>,
    active: bool,
    snapshot_boundary: &str,
) -> Value {
    json!({
        "id": id,
        "status": status,
        "originalSizeBytes": original_size,
        "exportedSizeBytes": exported_size,
        "activeSnapshot": active,
        "snapshotBoundary": snapshot_boundary,
    })
}

fn json_bytes(value: &Value) -> Result<Vec<u8>, RuntimeError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| RuntimeError::Storage(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn safe_segment(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    value.chars().take(120).collect()
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
        })
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn system_time_millis(value: SystemTime) -> String {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
#[path = "support_export_tests.rs"]
mod tests;
