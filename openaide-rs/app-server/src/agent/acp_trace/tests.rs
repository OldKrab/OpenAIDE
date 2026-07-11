use agent_client_protocol::LineDirection;

use super::naming::trace_enabled;
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
