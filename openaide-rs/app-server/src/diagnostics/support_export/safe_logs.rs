use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::{json, Value};

use super::safe_token;

const MAX_SAFE_LOG_BYTES: u64 = 2 * 1024 * 1024;

pub(super) fn safe_log_snapshot(path: &Path) -> std::io::Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    let start = metadata.len().saturating_sub(MAX_SAFE_LOG_BYTES);
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    if start > 0 {
        text = text
            .split_once('\n')
            .map(|(_, tail)| tail.to_string())
            .unwrap_or_default();
    }
    let mut output = String::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(object) = value.as_object() else {
            continue;
        };
        let Some(event) = object
            .get("event")
            .and_then(Value::as_str)
            .filter(|value| safe_token(value))
        else {
            continue;
        };
        let scope = object
            .get("scope")
            .and_then(Value::as_str)
            .filter(|value| safe_token(value))
            .unwrap_or("openaide");
        let level = object
            .get("level")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "info" | "warn" | "error"))
            .unwrap_or("info");
        let timestamp = object
            .get("timestamp")
            .or_else(|| object.get("timestamp_ms"))
            .cloned()
            .unwrap_or(Value::Null);
        let fields = object
            .get("fields")
            .and_then(Value::as_object)
            .map(safe_log_fields)
            .unwrap_or_default();
        output.push_str(&serde_json::to_string(&json!({
            "timestamp": timestamp,
            "scope": scope,
            "level": level,
            "event": event,
            "fields": fields,
        }))?);
        output.push('\n');
    }
    Ok(output.into_bytes())
}

/// Keeps only metadata fields needed to reconstruct lifecycle decisions. Free-form
/// strings and environment-bearing fields never cross the support-export boundary.
fn safe_log_fields(fields: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    fields
        .iter()
        .filter_map(|(name, value)| {
            let allowed = matches!(
                name.as_str(),
                "event"
                    | "surface"
                    | "project_id"
                    | "agent_id"
                    | "task_id"
                    | "outcome"
                    | "selection_source"
                    | "client_identity_source"
                    | "extension_version"
                    | "duration_ms"
            ) || name.ends_with("_present")
                || name.ends_with("_valid")
                || name.ends_with("_seeded");
            if !allowed {
                return None;
            }
            let safe_value = match value {
                Value::String(token) if safe_token(token) => value.clone(),
                Value::Bool(_) | Value::Number(_) | Value::Null => value.clone(),
                _ => return None,
            };
            Some((name.clone(), safe_value))
        })
        .collect()
}
