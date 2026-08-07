use agent_client_protocol::LineDirection;
use std::collections::HashSet;
use std::time::Duration;

use super::naming::trace_enabled;
use super::retention::TracePolicy;
use super::{AcpTraceSession, AcpTraceState};

#[test]
fn trace_enabled_accepts_explicit_developer_values() {
    assert!(trace_enabled(Some("1")));
    assert!(trace_enabled(Some("true")));
    assert!(trace_enabled(Some("raw")));
    assert!(!trace_enabled(None));
    assert!(!trace_enabled(Some("0")));
}

#[test]
fn trace_session_writes_only_after_enabled() {
    let tmp = tempfile::TempDir::new().unwrap();
    let state = AcpTraceState::disabled(tmp.path());
    let trace = AcpTraceSession::new(state.clone(), "task/test", "start");
    trace.record_line(
        r#"{"jsonrpc":"2.0","method":"unknown"}"#,
        LineDirection::Stdout,
    );
    assert!(!tmp.path().join("diagnostics/acp-traces").exists());

    state.set_enabled(true).unwrap();
    trace.record_line(
        r#"{"jsonrpc":"2.0","method":"unknown"}"#,
        LineDirection::Stdout,
    );

    let dir = tmp.path().join("diagnostics/acp-traces");
    let entry = std::fs::read_dir(dir).unwrap().next().unwrap().unwrap();
    let text = std::fs::read_to_string(entry.path()).unwrap();
    assert!(text.contains("\"trace_opened\""));
    assert!(text.contains("\"raw_line\""));
    assert!(text.contains("\"agent_to_client.raw_stdout\""));
    assert!(text.contains("unknown"));
}

#[test]
fn trace_file_stays_bounded_and_ends_with_a_complete_truncation_record() {
    let tmp = tempfile::TempDir::new().unwrap();
    let max_file_bytes = 1_024;
    let state = AcpTraceState::disabled_with_policy(
        tmp.path(),
        TracePolicy {
            max_file_bytes,
            max_total_bytes: 16 * 1_024,
            max_age: Duration::from_secs(60),
        },
    );
    state.set_enabled(true).unwrap();
    let trace = AcpTraceSession::new(state, "task_1", "bounded");

    for _ in 0..100 {
        trace.record("agent_to_client", "small", &serde_json::json!({}));
    }
    drop(trace);

    let directory = tmp.path().join("diagnostics/acp-traces");
    let path = std::fs::read_dir(directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(std::fs::metadata(&path).unwrap().len() <= max_file_bytes);
    let lines = std::fs::read_to_string(path).unwrap();
    let records = lines
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        records.last().and_then(|record| record["event"].as_str()),
        Some("trace_truncated")
    );
}

#[test]
fn enabling_trace_prunes_oldest_closed_files_to_the_total_budget() {
    let tmp = tempfile::TempDir::new().unwrap();
    let directory = tmp.path().join("diagnostics/acp-traces");
    std::fs::create_dir_all(&directory).unwrap();
    let oldest = directory.join("oldest.jsonl");
    let newest = directory.join("newest.jsonl");
    std::fs::write(&oldest, vec![b'a'; 600]).unwrap();
    std::thread::sleep(Duration::from_millis(10));
    std::fs::write(&newest, vec![b'b'; 600]).unwrap();

    let state = AcpTraceState::disabled_with_policy(
        tmp.path(),
        TracePolicy {
            max_file_bytes: 1_024,
            max_total_bytes: 600,
            max_age: Duration::from_secs(60),
        },
    );
    state.set_enabled(true).unwrap();

    assert!(!oldest.exists());
    assert!(newest.exists());
}

#[test]
fn disabled_trace_state_still_prunes_existing_files_on_startup() {
    let tmp = tempfile::TempDir::new().unwrap();
    let directory = tmp.path().join("diagnostics/acp-traces");
    std::fs::create_dir_all(&directory).unwrap();
    let oldest = directory.join("oldest.jsonl");
    let newest = directory.join("newest.jsonl");
    std::fs::write(&oldest, vec![b'a'; 600]).unwrap();
    std::thread::sleep(Duration::from_millis(10));
    std::fs::write(&newest, vec![b'b'; 600]).unwrap();

    let state = AcpTraceState::disabled_with_policy(
        tmp.path(),
        TracePolicy {
            max_file_bytes: 1_024,
            max_total_bytes: 600,
            max_age: Duration::from_secs(60),
        },
    );

    assert!(!state.status().enabled);
    assert!(!oldest.exists());
    assert!(newest.exists());
}

#[test]
fn pending_trace_bytes_remain_counted_during_another_retention_scan() {
    let tmp = tempfile::TempDir::new().unwrap();
    let state = AcpTraceState::disabled_with_policy(
        tmp.path(),
        TracePolicy {
            max_file_bytes: 1_024,
            max_total_bytes: 1_000,
            max_age: Duration::from_secs(60),
        },
    );
    state.set_enabled(true).unwrap();
    let first = state.open_trace_file("task-1", "first").unwrap();
    let second = state.open_trace_file("task-2", "second").unwrap();

    assert!(state.reserve_bytes(first.path(), 600));
    assert!(!state.reserve_bytes(second.path(), 600));

    state.release_bytes(600);
}

#[test]
fn closed_trace_older_than_seven_days_is_pruned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let trace = tmp.path().join("expired.jsonl");
    std::fs::write(&trace, b"closed trace").unwrap();
    let now = std::time::SystemTime::now() + Duration::from_secs(8 * 24 * 60 * 60);

    super::retention::prune(tmp.path(), &HashSet::new(), TracePolicy::default(), now).unwrap();

    assert!(!trace.exists());
}
