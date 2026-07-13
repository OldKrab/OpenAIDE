use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::acp_schema::{
    CloseSessionRequest, CloseSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    SessionId,
};
use agent_client_protocol::{Agent, Client};

use crate::agent::acp_trace::{AcpTraceSession, AcpTraceState};

#[derive(Clone)]
struct TerminationTestAgent {
    close_count: Arc<AtomicUsize>,
    delete_count: Arc<AtomicUsize>,
    fail_delete: bool,
}

impl agent_client_protocol::ConnectTo<Client> for TerminationTestAgent {
    fn connect_to(
        self,
        client: impl agent_client_protocol::ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = agent_client_protocol::Result<()>> + Send {
        let close_count = self.close_count.clone();
        let delete_count = self.delete_count.clone();
        let fail_delete = self.fail_delete;

        Agent
            .builder()
            .name("termination-test-agent")
            .on_receive_request(
                async move |_request: CloseSessionRequest, responder, _connection| {
                    close_count.fetch_add(1, Ordering::SeqCst);
                    responder.respond(CloseSessionResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: DeleteSessionRequest, responder, _connection| {
                    delete_count.fetch_add(1, Ordering::SeqCst);
                    if fail_delete {
                        responder.respond_with_error(agent_client_protocol::Error::new(
                            -32603,
                            "delete exploded",
                        ))
                    } else {
                        responder.respond(DeleteSessionResponse::new())
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(client)
    }
}

fn session_id() -> SessionId {
    SessionId::from("session-under-test")
}

fn run_with_agent(
    fail_delete: bool,
    run: impl AsyncFnOnce(
        ConnectionTo<Agent>,
        Arc<AtomicUsize>,
        Arc<AtomicUsize>,
    ) -> agent_client_protocol::Result<()>,
) {
    let close_count = Arc::new(AtomicUsize::new(0));
    let delete_count = Arc::new(AtomicUsize::new(0));
    let agent = TerminationTestAgent {
        close_count: close_count.clone(),
        delete_count: delete_count.clone(),
        fail_delete,
    };

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        Client
            .builder()
            .connect_with(agent, |connection| {
                run(connection, close_count.clone(), delete_count.clone())
            })
            .await
            .unwrap();
    });
}

fn enabled_trace(temp: &tempfile::TempDir) -> AcpTraceSession {
    let state = AcpTraceState::disabled(temp.path());
    state.set_enabled(true).expect("enable trace");
    AcpTraceSession::new(state, "task-termination", "termination-test")
}

fn wait_for_trace_content(trace_root: &std::path::Path) -> String {
    let started = Instant::now();
    loop {
        if let Ok(entries) = std::fs::read_dir(trace_root) {
            for entry in entries.flatten() {
                let content = std::fs::read_to_string(entry.path()).expect("trace content");
                if content.contains("session/") {
                    return content;
                }
            }
        }
        if started.elapsed() > Duration::from_secs(1) {
            panic!("trace content was not written");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_trace_pair(content: &str, event: &str, direction: &str) {
    let found = content.lines().any(|line| {
        let value: serde_json::Value = serde_json::from_str(line).expect("trace json line");
        value.get("event").and_then(serde_json::Value::as_str) == Some(event)
            && value.get("direction").and_then(serde_json::Value::as_str) == Some(direction)
    });
    assert!(
        found,
        "missing trace pair event={event} direction={direction} in {content}"
    );
}

#[test]
fn unsupported_close_is_noop_without_trace_or_agent_request() {
    run_with_agent(false, async |connection, close_count, _delete_count| {
        close_active_session(&connection, session_id(), false, None).await;
        assert_eq!(close_count.load(Ordering::SeqCst), 0);
        Ok(())
    });
}

#[test]
fn unsupported_delete_returns_stable_capability_error_without_agent_request() {
    run_with_agent(false, async |connection, _close_count, delete_count| {
        let error = delete_active_session(&connection, session_id(), false, None)
            .await
            .expect_err("delete should require capability");
        assert_eq!(
            error.to_string(),
            "capability missing: agent session delete is not available"
        );
        assert_eq!(delete_count.load(Ordering::SeqCst), 0);
        Ok(())
    });
}

#[test]
fn close_records_request_and_response_trace_events() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let trace = enabled_trace(&temp);

    run_with_agent(false, async |connection, close_count, _delete_count| {
        close_active_session(&connection, session_id(), true, Some(&trace)).await;
        assert_eq!(close_count.load(Ordering::SeqCst), 1);
        Ok(())
    });

    let content = wait_for_trace_content(&temp.path().join("diagnostics/acp-traces"));
    assert_trace_pair(&content, "session/close.request", "client_to_agent");
    assert_trace_pair(&content, "session/close.response", "agent_to_client");
}

#[test]
fn failed_delete_records_error_trace_and_maps_acp_error() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let trace = enabled_trace(&temp);

    run_with_agent(true, async |connection, _close_count, delete_count| {
        let error = delete_active_session(&connection, session_id(), true, Some(&trace))
            .await
            .expect_err("delete should map ACP error");
        assert!(error.to_string().contains("ACP error"), "{error}");
        assert_eq!(delete_count.load(Ordering::SeqCst), 1);
        Ok(())
    });

    let content = wait_for_trace_content(&temp.path().join("diagnostics/acp-traces"));
    assert_trace_pair(&content, "session/delete.request", "client_to_agent");
    assert_trace_pair(&content, "session/delete.error", "agent_to_client");
    assert!(content.contains("delete exploded"));
}
