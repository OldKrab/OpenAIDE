use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::logging;
use crate::time::now_string;

use super::naming::{compact_timestamp, safe_file_segment};

pub(super) struct TraceFile {
    path: PathBuf,
    file: std::fs::File,
}

pub(super) fn open_trace_file(root: &Path, task_id: &str, operation: &str) -> Option<TraceFile> {
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
    let mut trace_file = TraceFile { path, file };
    write_trace_opened(&mut trace_file, task_id, operation);
    Some(trace_file)
}

pub(super) fn write_json_line(trace_file: &mut TraceFile, value: &Value) {
    if serde_json::to_writer(&mut trace_file.file, value).is_ok() {
        let _ = writeln!(&mut trace_file.file);
        let _ = trace_file.file.flush();
    }
}

fn write_trace_opened(trace_file: &mut TraceFile, task_id: &str, operation: &str) {
    let opened = json!({
        "at": now_string(),
        "direction": "runtime",
        "event": "trace_opened",
        "sensitive": true,
        "payload": {
            "task_id": task_id,
            "operation": operation,
            "trace_path": trace_file.path.to_string_lossy(),
        },
    });
    write_json_line(trace_file, &opened);
}
