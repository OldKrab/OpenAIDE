use std::sync::{Arc, Mutex};

use agent_client_protocol::LineDirection;
use serde::Serialize;
use serde_json::{json, Value};

use crate::time::now_string;

use super::file::{TraceFile, TraceWriteOutcome};
use super::state::AcpTraceState;

enum TraceSlot {
    Pending,
    Open(TraceFile),
    Stopped,
}

struct SharedTraceFile {
    state: AcpTraceState,
    slot: Mutex<TraceSlot>,
}

impl Drop for SharedTraceFile {
    fn drop(&mut self) {
        let slot = self.slot.get_mut().expect("ACP trace file lock poisoned");
        if let TraceSlot::Open(trace_file) = std::mem::replace(slot, TraceSlot::Stopped) {
            let path = trace_file.path().to_path_buf();
            drop(trace_file);
            // Closed files can now be pruned on every supported platform.
            self.state.close_trace_file(&path);
        }
    }
}

#[derive(Clone)]
pub struct AcpTraceSession {
    state: AcpTraceState,
    task_id: Arc<str>,
    operation: Arc<str>,
    file: Arc<SharedTraceFile>,
}

impl AcpTraceSession {
    pub fn new(state: AcpTraceState, task_id: &str, operation: &str) -> Self {
        Self {
            state: state.clone(),
            task_id: Arc::from(task_id),
            operation: Arc::from(operation),
            file: Arc::new(SharedTraceFile {
                state: state.clone(),
                slot: Mutex::new(TraceSlot::Pending),
            }),
        }
    }

    pub(crate) fn task_id(&self) -> &str {
        &self.task_id
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
        if !self.state.is_enabled() {
            return;
        }
        let mut guard = self.file.slot.lock().expect("ACP trace file lock poisoned");
        if matches!(*guard, TraceSlot::Pending) {
            let Some(trace_file) = self
                .state
                .open_trace_file(self.task_id.as_ref(), self.operation.as_ref())
            else {
                return;
            };
            let path = trace_file.path().to_string_lossy().to_string();
            *guard = TraceSlot::Open(trace_file);
            let opened = json!({
                "at": now_string(),
                "direction": "runtime",
                "event": "trace_opened",
                "sensitive": true,
                "payload": {
                    "task_id": self.task_id.as_ref(),
                    "operation": self.operation.as_ref(),
                    "trace_path": path,
                },
            });
            if !self.write_or_stop(&mut guard, &opened) {
                return;
            }
        }
        if matches!(*guard, TraceSlot::Stopped) {
            return;
        }
        let line = json!({
            "at": now_string(),
            "direction": direction,
            "event": event,
            "sensitive": true,
            "payload": payload,
        });
        self.write_or_stop(&mut guard, &line);
    }

    fn write_or_stop(&self, slot: &mut TraceSlot, value: &Value) -> bool {
        let (outcome, path) = match slot {
            TraceSlot::Open(trace_file) => {
                let outcome = trace_file.write_json_line(value, &self.state);
                if outcome == TraceWriteOutcome::FileLimit {
                    let marker = json!({
                        "at": now_string(),
                        "direction": "runtime",
                        "event": "trace_truncated",
                        "sensitive": true,
                        "payload": { "reason_code": "file_limit" },
                    });
                    let _ = trace_file.write_truncation_line(&marker, &self.state);
                }
                (outcome, trace_file.path().to_path_buf())
            }
            TraceSlot::Pending | TraceSlot::Stopped => return false,
        };
        if outcome == TraceWriteOutcome::Written {
            return true;
        }
        *slot = TraceSlot::Stopped;
        self.state.close_trace_file(&path);
        crate::logging::warn(
            "acp_trace_stopped",
            json!({ "reason_code": trace_outcome_name(outcome) }),
        );
        false
    }
}

fn trace_outcome_name(outcome: TraceWriteOutcome) -> &'static str {
    match outcome {
        TraceWriteOutcome::Written => "written",
        TraceWriteOutcome::FileLimit => "file_limit",
        TraceWriteOutcome::TotalLimit => "total_limit",
        TraceWriteOutcome::WriteFailed => "write_failed",
    }
}
