use serde_json::Value;

use crate::agent::tool_details_sanitizer::{
    command_input_summary, is_secret_key, is_sensitive_key, sanitize_command_detail,
    sanitize_command_summary, tool_input_field_summary,
};
use crate::protocol::model::{
    ActivityToolField, ActivityToolInput, ActivityToolOutput, ActivityToolValue,
};

pub(super) use crate::agent::tool_details_sanitizer::truncate_preview;

pub(super) fn tool_input_detail(value: &Value) -> Option<ActivityToolInput> {
    let Some(object) = value.as_object() else {
        return Some(ActivityToolInput {
            command: Vec::new(),
            cwd: None,
            query: None,
            queries: Vec::new(),
            url: None,
            path: None,
            fields: vec![ActivityToolField {
                name: "value".to_string(),
                value: normalized_tool_value(value),
            }],
        });
    };
    let (command_key, command) = command_field(object);
    let cwd = string_field(object.get("cwd"));
    let (query_key, query) = first_string_field(object, &["query", "q"]);
    let queries = web_search_queries(object.get("action"));
    let url = string_field(object.get("url"));
    let (path_key, path) = first_string_field(object, &["path", "file"]);
    let mut consumed = Vec::new();
    if let Some(key) = command_key {
        consumed.push(key);
    }
    if cwd.is_some() {
        consumed.push("cwd");
    }
    if let Some(key) = query_key {
        consumed.push(key);
    }
    if url.is_some() {
        consumed.push("url");
    }
    if let Some(key) = path_key {
        consumed.push(key);
    }
    let fields = normalized_fields(object, &consumed);
    if command.is_empty()
        && cwd.is_none()
        && query.is_none()
        && queries.is_empty()
        && url.is_none()
        && path.is_none()
        && fields.is_empty()
    {
        None
    } else {
        Some(ActivityToolInput {
            command,
            cwd,
            query,
            queries,
            url,
            path,
            fields,
        })
    }
}

pub(super) fn tool_output_detail(value: &Value) -> Option<ActivityToolOutput> {
    let Some(object) = value.as_object() else {
        return Some(ActivityToolOutput {
            stdout: None,
            stderr: None,
            formatted_output: None,
            aggregated_output: None,
            exit_code: None,
            success: None,
            fields: vec![ActivityToolField {
                name: "value".to_string(),
                value: normalized_tool_value(value),
            }],
        });
    };
    let stdout = string_field(object.get("stdout"));
    let stderr = string_field(object.get("stderr"));
    let formatted_output = string_field(object.get("formatted_output"));
    let aggregated_output = string_field(object.get("aggregated_output"));
    let exit_code = object
        .get("exit_code")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok());
    let success = object.get("success").and_then(Value::as_bool);
    let mut consumed = Vec::new();
    for (key, present) in [
        ("stdout", stdout.is_some()),
        ("stderr", stderr.is_some()),
        ("formatted_output", formatted_output.is_some()),
        ("aggregated_output", aggregated_output.is_some()),
        ("exit_code", exit_code.is_some()),
        ("success", success.is_some()),
    ] {
        if present {
            consumed.push(key);
        }
    }
    let fields = normalized_fields(object, &consumed);
    if stdout.is_none()
        && stderr.is_none()
        && formatted_output.is_none()
        && aggregated_output.is_none()
        && exit_code.is_none()
        && success.is_none()
        && fields.is_empty()
    {
        None
    } else {
        Some(ActivityToolOutput {
            stdout,
            stderr,
            formatted_output,
            aggregated_output,
            exit_code,
            success,
            fields,
        })
    }
}

pub(super) fn tool_input_summary(raw_input: Option<&Value>) -> Option<String> {
    let value = raw_input?;
    let object = value.as_object()?;
    if let Some(summary) =
        command_input_summary(object.get("cmd").or_else(|| object.get("command")))
    {
        return Some(summary);
    }

    const PRIORITY_KEYS: &[&str] = &["query", "q", "pattern", "url", "path", "file", "cwd"];

    for key in PRIORITY_KEYS {
        if let Some(summary) = object
            .get(*key)
            .and_then(|value| tool_input_field_summary(key, value))
        {
            return Some(summary);
        }
    }

    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    let summary = keys
        .into_iter()
        .filter_map(|key| {
            if is_sensitive_key(key) {
                return None;
            }
            object
                .get(key)
                .and_then(|value| tool_input_field_summary(key, value))
                .map(|value| format!("{key} {value}"))
        })
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
    if summary.is_empty() {
        None
    } else {
        Some(truncate_preview(summary))
    }
}

fn web_search_queries(action: Option<&Value>) -> Vec<String> {
    action
        .and_then(Value::as_object)
        .and_then(|action| action.get("queries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(sanitize_command_summary)
        .filter(|query| !query.is_empty())
        .take(8)
        .collect()
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn first_string_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&'a str],
) -> (Option<&'a str>, Option<String>) {
    keys.iter()
        .find_map(|key| string_field(object.get(*key)).map(|value| (*key, value)))
        .map_or((None, None), |(key, value)| (Some(key), Some(value)))
}

fn command_field(object: &serde_json::Map<String, Value>) -> (Option<&str>, Vec<String>) {
    for key in ["command", "cmd"] {
        let Some(value) = object.get(key) else {
            continue;
        };
        let command = match value {
            Value::String(command) if !command.is_empty() => vec![sanitize_command_detail(command)],
            Value::Array(items) => {
                let command = items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                let command = sanitize_command_detail(&command);
                (!command.is_empty())
                    .then_some(command)
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        };
        if !command.is_empty() {
            return (Some(key), command);
        }
    }
    (None, Vec::new())
}

fn normalized_fields(
    object: &serde_json::Map<String, Value>,
    excluded: &[&str],
) -> Vec<ActivityToolField> {
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    keys.into_iter()
        .filter(|key| !excluded.iter().any(|excluded| excluded == &key.as_str()))
        .filter_map(|key| {
            let value = object.get(key)?;
            if is_secret_key(key) {
                return Some(ActivityToolField {
                    name: key.clone(),
                    value: ActivityToolValue::Redacted,
                });
            }
            Some(ActivityToolField {
                name: key.clone(),
                value: normalized_tool_value(value),
            })
        })
        .collect()
}

fn normalized_tool_value(value: &Value) -> ActivityToolValue {
    match value {
        Value::Null => ActivityToolValue::Null,
        Value::Bool(value) => ActivityToolValue::Boolean { value: *value },
        Value::Number(value) => ActivityToolValue::Number {
            value: value.to_string(),
        },
        Value::String(value) => ActivityToolValue::String {
            value: value.clone(),
        },
        Value::Array(items) => ActivityToolValue::Array {
            items: items.iter().map(normalized_tool_value).collect(),
        },
        Value::Object(object) => ActivityToolValue::Object {
            fields: normalized_fields(object, &[]),
        },
    }
}
