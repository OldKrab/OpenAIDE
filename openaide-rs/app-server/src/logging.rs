use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

struct LoggerState {
    file: Option<File>,
    write_failure_reported: bool,
}

static LOGGER: OnceLock<Mutex<LoggerState>> = OnceLock::new();

pub fn init_file_logger(storage_root: &Path) {
    let path = storage_root
        .join("diagnostics")
        .join("logs")
        .join("openaide-app-server.jsonl");
    let file = fs::create_dir_all(path.parent().unwrap_or(storage_root))
        .and_then(|_| OpenOptions::new().create(true).append(true).open(path));
    if file.is_err() {
        write_fallback("app_server_log_open_failed");
    }
    let logger = LOGGER.get_or_init(|| {
        Mutex::new(LoggerState {
            file: None,
            write_failure_reported: false,
        })
    });
    *logger.lock().expect("app server logger lock poisoned") = LoggerState {
        file: file.ok(),
        write_failure_reported: false,
    };
}

pub fn info(event: &str, fields: Value) {
    write("info", event, fields);
}

pub fn warn(event: &str, fields: Value) {
    write("warn", event, fields);
}

pub fn error(event: &str, fields: Value) {
    write("error", event, fields);
}

fn write(level: &str, event: &str, fields: Value) {
    let line = json!({
        "timestamp_ms": timestamp_ms(),
        "scope": "openaide-app-server",
        "level": level,
        "event": event,
        "fields": sanitize_value(fields),
    });
    let Ok(text) = serde_json::to_string(&line) else {
        return;
    };
    let Some(logger) = LOGGER.get() else {
        return;
    };
    let mut guard = logger.lock().expect("runtime logger lock poisoned");
    if let Some(file) = guard.file.as_mut() {
        let result = writeln!(file, "{text}").and_then(|_| file.flush());
        if result.is_err() && !guard.write_failure_reported {
            guard.write_failure_reported = true;
            write_fallback("app_server_log_write_failed");
        }
    }
}

fn write_fallback(event: &str) {
    // stderr is the last-resort diagnostic path and carries no runtime payload.
    eprintln!(
        "{}",
        json!({
            "timestamp_ms": timestamp_ms(),
            "scope": "openaide-app-server",
            "level": "error",
            "event": event,
            "fields": {},
        })
    );
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn sanitize_value(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(sanitize_text(&text)),
        Value::Array(values) => Value::Array(values.into_iter().map(sanitize_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    if is_sensitive_key(&key) {
                        // Free-form diagnostic values cannot be made safe with keyword
                        // replacement: arbitrary user text may contain no recognizable marker.
                        (key, Value::String("[redacted]".to_string()))
                    } else {
                        (key, sanitize_value(value))
                    }
                })
                .collect(),
        ),
        other => other,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    if ["_kind", "_code", "_count", "_bytes", "_status"]
        .iter()
        .any(|suffix| key.ends_with(suffix))
    {
        return false;
    }
    [
        "prompt",
        "secret",
        "token",
        "password",
        "content",
        "output",
        "path",
        "message",
        "error",
        "command",
        "cwd",
        "environment",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn sanitize_text(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            if is_path_like(word) || contains_sensitive_word(word) {
                "[redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_path_like(word: &str) -> bool {
    let trimmed = word.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\'' | '`' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}'
        )
    });
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return true;
    }
    let mut chars = trimmed.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(drive), Some(':'), Some(sep)) if drive.is_ascii_alphabetic() && (sep == '/' || sep == '\\')
    )
}

fn contains_sensitive_word(word: &str) -> bool {
    let word = word.to_ascii_lowercase();
    [
        "prompt", "secret", "token", "password", "content", "output", "path",
    ]
    .iter()
    .any(|needle| word.contains(needle))
}

#[cfg(test)]
#[path = "logging_tests.rs"]
mod tests;
