use std::sync::{Arc, Mutex};

use agent_client_protocol::LineDirection;
use serde::Serialize;
use serde_json::{json, Value};

use crate::time::now_string;

use super::file::{open_trace_file, write_json_line, TraceFile};
use super::state::AcpTraceState;

#[derive(Clone)]
pub struct AcpTraceSession {
    state: AcpTraceState,
    task_id: Arc<str>,
    operation: Arc<str>,
    file: Arc<Mutex<Option<TraceFile>>>,
}

impl AcpTraceSession {
    pub fn new(state: AcpTraceState, task_id: &str, operation: &str) -> Self {
        Self {
            state,
            task_id: Arc::from(task_id),
            operation: Arc::from(operation),
            file: Arc::new(Mutex::new(None)),
        }
    }

    pub fn record_line(&self, line: &str, direction: LineDirection) {
        let direction = match direction {
            LineDirection::Stdout => "agent_to_client.raw_stdout",
            LineDirection::Stdin => "client_to_agent.raw_stdin",
            LineDirection::Stderr => "agent_to_client.raw_stderr",
        };
        self.record_value(direction, "raw_line", json!({ "line": line }));
    }

    pub fn record<T: Serialize>(&self, direction: &str, event: &str, payload: &T) {
        match serde_json::to_value(payload) {
            Ok(value) => self.record_value(direction, event, value),
            Err(error) => self.record_value(
                "runtime",
                "trace_serialize_failed",
                json!({ "event": event, "error": error.to_string() }),
            ),
        }
    }

    pub fn record_value(&self, direction: &str, event: &str, payload: Value) {
        let Some(root) = self.state.enabled_root() else {
            return;
        };
        let mut guard = self.file.lock().expect("ACP trace file lock poisoned");
        if guard.is_none() {
            *guard = open_trace_file(&root, self.task_id.as_ref(), self.operation.as_ref());
        }
        let Some(trace_file) = guard.as_mut() else {
            return;
        };
        let line = json!({
            "at": now_string(),
            "direction": direction,
            "event": event,
            "sensitive": true,
            "payload": payload,
        });
        write_json_line(trace_file, &line);
    }
}
