use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::logging;

use super::naming::{compact_timestamp, safe_file_segment};
use super::state::AcpTraceState;

// Ordinary records leave enough room for one final structured reason instead
// of ending a capped sensitive trace with an ambiguous partial lifecycle.
const TRUNCATION_RECORD_RESERVE_BYTES: u64 = 512;

pub(super) struct TraceFile {
    path: PathBuf,
    file: std::fs::File,
    bytes: u64,
    max_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TraceWriteOutcome {
    Written,
    FileLimit,
    TotalLimit,
    WriteFailed,
}

pub(super) fn create_trace_file(
    root: &Path,
    task_id: &str,
    operation: &str,
    max_bytes: u64,
) -> Option<TraceFile> {
    if let Err(error) = fs::create_dir_all(root) {
        logging::warn(
            "acp_trace_create_dir_failed",
            json!({ "error": error.to_string() }),
        );
        eprintln!("OpenAIDE ACP trace disabled: cannot create trace dir: {error}");
        return None;
    }
    let file_name = format!(
        "{}-{}-{}.jsonl",
        compact_timestamp(),
        safe_file_segment(task_id),
        safe_file_segment(operation)
    );
    let path = root.join(file_name);
    let file = match OpenOptions::new().create_new(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) => {
            logging::warn(
                "acp_trace_open_failed",
                json!({ "error": error.to_string() }),
            );
            eprintln!("OpenAIDE ACP trace disabled: cannot open trace file: {error}");
            return None;
        }
    };
    Some(TraceFile {
        path,
        file,
        bytes: 0,
        max_bytes,
    })
}

impl TraceFile {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write_json_line(&mut self, value: &Value, state: &AcpTraceState) -> TraceWriteOutcome {
        self.write_json_line_with_reserve(value, state, TRUNCATION_RECORD_RESERVE_BYTES)
    }

    pub fn write_truncation_line(
        &mut self,
        value: &Value,
        state: &AcpTraceState,
    ) -> TraceWriteOutcome {
        self.write_json_line_with_reserve(value, state, 0)
    }

    fn write_json_line_with_reserve(
        &mut self,
        value: &Value,
        state: &AcpTraceState,
        reserve_bytes: u64,
    ) -> TraceWriteOutcome {
        let mut encoded = match serde_json::to_vec(value) {
            Ok(encoded) => encoded,
            Err(_) => return TraceWriteOutcome::WriteFailed,
        };
        encoded.push(b'\n');
        let encoded_bytes = u64::try_from(encoded.len()).unwrap_or(u64::MAX);
        let ordinary_limit = self.max_bytes.saturating_sub(reserve_bytes);
        if self.bytes.saturating_add(encoded_bytes) > ordinary_limit {
            return TraceWriteOutcome::FileLimit;
        }
        if !state.reserve_bytes(&self.path, encoded_bytes) {
            return TraceWriteOutcome::TotalLimit;
        }
        if self
            .file
            .write_all(&encoded)
            .and_then(|_| self.file.flush())
            .is_err()
        {
            state.release_bytes(encoded_bytes);
            return TraceWriteOutcome::WriteFailed;
        }
        state.commit_bytes(encoded_bytes);
        self.bytes = self.bytes.saturating_add(encoded_bytes);
        TraceWriteOutcome::Written
    }
}
