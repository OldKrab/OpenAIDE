use std::fs;

use serde_json::json;

use super::{complete_jsonl_tail, complete_prefix, trace_identity};

#[test]
fn complete_prefix_excludes_a_live_partial_record() {
    let directory = tempfile::TempDir::new().expect("temporary directory");
    let path = directory.path().join("active.jsonl");
    fs::write(&path, b"{\"complete\":true}\n{\"partial\":").expect("write fixture");

    let (bytes, truncated) = complete_prefix(&path).expect("snapshot");

    assert_eq!(bytes, b"{\"complete\":true}\n");
    assert!(truncated);
}

#[test]
fn complete_jsonl_tail_keeps_only_the_newest_complete_records() {
    let directory = tempfile::TempDir::new().expect("temporary directory");
    let path = directory.path().join("large.jsonl");
    fs::write(&path, b"first\nsecond\nthird\npartial").expect("write fixture");

    let (bytes, truncated) = complete_jsonl_tail(&path, 13).expect("snapshot");

    assert_eq!(bytes, b"second\nthird\n");
    assert!(truncated);
}

#[test]
fn trace_identity_uses_raw_acp_session_id_instead_of_recency() {
    let directory = tempfile::TempDir::new().expect("temporary directory");
    let path = directory.path().join("trace.jsonl");
    fs::write(
        &path,
        [
            json!({"event":"trace_opened","payload":{"task_id":"task-1","operation":"session-resume"}}).to_string(),
            json!({"event":"client_to_agent.raw_stdin","payload":{"line":"{\"method\":\"session/resume\",\"params\":{\"sessionId\":\"native-1\"}}"}}).to_string(),
        ]
        .join("\n")
            + "\n",
    )
    .expect("write trace");

    assert_eq!(
        trace_identity(&path),
        (
            Some("task-1".to_string()),
            Some("native-1".to_string()),
            "session-resume".to_string()
        ),
    );
}
