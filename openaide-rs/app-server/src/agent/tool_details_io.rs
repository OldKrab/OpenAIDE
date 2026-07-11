use serde_json::Value;

use crate::agent::tool_details_sanitizer::{
    command_input_summary, is_sensitive_key, path_leaf_summary, sanitize_command_summary,
    tool_input_field_summary,
};
use crate::protocol::model::{ActivityToolField, ActivityToolInput, ActivityToolOutput};

pub(super) use crate::agent::tool_details_sanitizer::truncate_preview;

pub(super) fn tool_input_detail(value: &Value) -> Option<ActivityToolInput> {
    let object = value.as_object()?;
    let command = string_array_field(object.get("command"));
    let cwd = string_field(object.get("cwd")).map(|value| path_leaf_summary(&value));
    let query = string_field(object.get("query").or_else(|| object.get("q")))
        .map(|value| sanitize_command_summary(&value));
    let queries = web_search_queries(object.get("action"));
    let url = string_field(object.get("url")).map(|value| sanitize_command_summary(&value));
    let path = string_field(object.get("path").or_else(|| object.get("file")))
        .map(|value| path_leaf_summary(&value));
    let fields = sanitized_scalar_fields(
        object,
        &[
            "command",
            "cwd",
            "query",
            "q",
            "url",
            "path",
            "file",
            "changes",
            "content",
            "output",
            "stdout",
            "stderr",
            "formatted_output",
            "aggregated_output",
            "call_id",
            "turn_id",
            "process_id",
            "started_at_ms",
            "completed_at_ms",
            "duration",
            "source",
            "parsed_cmd",
        ],
    );
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
    let object = value.as_object()?;
    let stdout = string_field(object.get("stdout"));
    let stderr = string_field(object.get("stderr"));
    let formatted_output = string_field(object.get("formatted_output"));
    let aggregated_output = string_field(object.get("aggregated_output"));
    let exit_code = object
        .get("exit_code")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok());
    let success = object.get("success").and_then(Value::as_bool);
    let fields = sanitized_scalar_fields(
        object,
        &[
            "stdout",
            "stderr",
            "formatted_output",
            "aggregated_output",
            "exit_code",
            "success",
            "changes",
            "content",
            "output",
            "call_id",
            "turn_id",
            "process_id",
            "started_at_ms",
            "completed_at_ms",
            "duration",
            "source",
            "parsed_cmd",
        ],
    );
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

fn string_array_field(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(sanitize_command_summary)
                .filter(|value| !value.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn sanitized_scalar_fields(
    object: &serde_json::Map<String, Value>,
    excluded: &[&str],
) -> Vec<ActivityToolField> {
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    keys.into_iter()
        .filter(|key| !excluded.iter().any(|excluded| excluded == &key.as_str()))
        .filter_map(|key| {
            if is_sensitive_key(key) {
                return Some(ActivityToolField {
                    name: key.clone(),
                    value: "[redacted]".to_string(),
                });
            }
            let value = object.get(key)?;
            tool_input_field_summary(key, value).map(|summary| ActivityToolField {
                name: key.clone(),
                value: summary,
            })
        })
        .take(12)
        .collect()
}
