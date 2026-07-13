use serde_json::Value;

pub(super) fn truncate_preview(value: String) -> String {
    const MAX_CHARS: usize = 180;
    if value.chars().count() <= MAX_CHARS {
        return value;
    }
    let mut truncated = value.chars().take(MAX_CHARS).collect::<String>();
    truncated.push_str("...");
    truncated
}

pub(super) fn command_input_summary(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(command) => Some(sanitize_command_summary(command)),
        Value::Array(command) => {
            command_array_summary(command).map(|command| sanitize_command_summary(&command))
        }
        _ => None,
    }
    .filter(|summary| !summary.is_empty())
}

pub(super) fn tool_input_field_summary(key: &str, value: &Value) -> Option<String> {
    if is_sensitive_key(key) {
        return Some("[redacted]".to_string());
    }
    match value {
        Value::String(text) => Some(match key {
            "cmd" | "command" => sanitize_command_summary(text),
            "path" | "file" | "cwd" => path_leaf_summary(text),
            _ => sanitize_scalar_summary(text),
        }),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
    .filter(|summary| !summary.is_empty())
}

pub(super) fn sanitize_command_summary(command: &str) -> String {
    truncate_preview(sanitize_command(command, true))
}

pub(super) fn sanitize_command_detail(command: &str) -> String {
    sanitize_command(command, false)
}

fn sanitize_command(command: &str, compact_paths: bool) -> String {
    let mut output = Vec::new();
    let mut redact_next = false;
    for part in command.split_whitespace() {
        let lower = part.to_ascii_lowercase();
        if redact_next {
            output.push("[redacted]".to_string());
            redact_next = false;
            continue;
        }
        if let Some((name, _)) = part.split_once('=') {
            if is_sensitive_key(&name.to_ascii_lowercase()) {
                output.push(format!("{name}=[redacted]"));
                continue;
            }
        }
        if is_sensitive_key(&lower) {
            output.push(part.to_string());
            redact_next = true;
            continue;
        }
        if compact_paths && looks_path_like(part) {
            output.push(path_leaf_summary(part));
        } else {
            output.push(part.to_string());
        }
    }
    output.join(" ")
}

pub(super) fn path_leaf_summary(value: &str) -> String {
    value
        .trim_matches(|ch| matches!(ch, '\'' | '"' | '`'))
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("[path]")
        .to_string()
}

pub(super) fn is_sensitive_key(key: &str) -> bool {
    is_secret_key(key)
        || matches!(
            key.to_ascii_lowercase().as_str(),
            "prompt" | "content" | "output"
        )
}

/// Raw detail trees keep ordinary content while redacting credential-bearing fields.
pub(super) fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("authorization")
        || lower == "auth"
        || lower == "key"
        || lower == "env"
}

fn command_array_summary(command: &[Value]) -> Option<String> {
    let parts = command
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    if parts.len() >= 3 && is_shell_launcher(parts[0]) && parts[1] == "-lc" {
        return Some(parts[2..].join(" "));
    }
    Some(parts.join(" "))
}

fn is_shell_launcher(value: &str) -> bool {
    let leaf = path_leaf_summary(value).to_ascii_lowercase();
    matches!(leaf.as_str(), "sh" | "bash" | "zsh")
}

fn sanitize_scalar_summary(value: &str) -> String {
    let trimmed = value.trim();
    if looks_path_like(trimmed) {
        return path_leaf_summary(trimmed);
    }
    sanitize_command_summary(trimmed)
}

fn looks_path_like(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("~/")
        || value.contains("/home/")
        || value.contains("\\Users\\")
        || value.contains(":/")
        || value.contains(":\\")
}
