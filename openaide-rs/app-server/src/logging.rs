use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

static LOGGER: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn init_file_logger(storage_root: &Path) {
    let path = storage_root
        .join("diagnostics")
        .join("logs")
        .join("openaide-app-server.jsonl");
    let file = fs::create_dir_all(path.parent().unwrap_or(storage_root))
        .and_then(|_| OpenOptions::new().create(true).append(true).open(path));
    let logger = LOGGER.get_or_init(|| Mutex::new(None));
    *logger.lock().expect("app server logger lock poisoned") = file.ok();
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
    if let Some(file) = guard.as_mut() {
        let _ = writeln!(file, "{text}");
        let _ = file.flush();
    }
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
                        (key, Value::String(sanitize_text(&value_to_text(&value))))
                    } else {
                        (key, sanitize_value(value))
                    }
                })
                .collect(),
        ),
        other => other,
    }
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "prompt", "secret", "token", "password", "content", "output", "path", "message", "error",
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
mod tests;
