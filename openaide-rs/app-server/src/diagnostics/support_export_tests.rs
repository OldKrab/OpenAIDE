use std::fs;

use serde_json::json;

use super::{complete_jsonl_tail, complete_prefix, safe_log_snapshot, trace_identity};

#[test]
fn safe_log_snapshot_keeps_selection_evidence_and_removes_sensitive_fields() {
    let directory = tempfile::TempDir::new().expect("temporary directory");
    let path = directory.path().join("app-server.jsonl");
    fs::write(
        &path,
        json!({
            "timestamp": 42,
            "scope": "vscode-extension",
            "level": "info",
            "event": "new_task_initial_project_selected",
            "fields": {
                "project_id": "project-safe",
                "agent_id": "codex",
                "task_id": "task-safe",
                "outcome": "updated",
                "selection_source": "app_server_default",
                "extension_version": "0.3.0-gabcdef0",
                "shell_project_present": false,
                "retained_project_present": true,
                "default_project_valid": true,
                "duration_ms": 12,
                "workspace_root": "/private/workspace",
                "error_message": "contains private text"
            }
        })
        .to_string()
            + "\n",
    )
    .expect("write fixture");

    let snapshot =
        String::from_utf8(safe_log_snapshot(&path).expect("safe snapshot")).expect("utf8 snapshot");
    let value: serde_json::Value = serde_json::from_str(snapshot.trim()).expect("log record");

    assert_eq!(value["fields"]["project_id"], "project-safe");
    assert_eq!(value["fields"]["agent_id"], "codex");
    assert_eq!(value["fields"]["task_id"], "task-safe");
    assert_eq!(value["fields"]["outcome"], "updated");
    assert_eq!(value["fields"]["selection_source"], "app_server_default");
    assert_eq!(value["fields"]["extension_version"], "0.3.0-gabcdef0");
    assert_eq!(value["fields"]["retained_project_present"], true);
    assert_eq!(value["fields"]["default_project_valid"], true);
    assert_eq!(value["fields"]["duration_ms"], 12);
    assert!(value["fields"].get("workspace_root").is_none());
    assert!(value["fields"].get("error_message").is_none());
}

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
