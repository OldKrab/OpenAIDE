//! Renderer diagnostics that must outlive the WebView console.
//!
//! The renderer's console is gone once the window reloads and is never in a file a
//! user can attach, so events that explain renderer-owned behavior, such as Task
//! notifications, cross into the Desktop log here. The renderer is trusted code, but
//! each value is still reduced to one token so a line cannot be split, extended, or
//! made to carry free text.

use std::collections::BTreeMap;

const MAX_TOKEN_CHARS: usize = 128;
const MAX_FIELDS: usize = 12;

#[tauri::command]
pub(crate) fn record_desktop_telemetry(payload: serde_json::Value) {
    let event = payload.get("event").map_or("unknown".to_string(), token);
    let surface = payload.get("surface").map_or("unknown".to_string(), token);
    desktop_log!("desktop_webview_event event={event} surface={surface}");
}

/// Writes one renderer event, named by `event`, with metadata-only `fields`.
#[tauri::command]
pub(crate) fn record_desktop_webview_log(
    event: String,
    fields: BTreeMap<String, serde_json::Value>,
) {
    desktop_log!("{}", webview_log_line(&event, &fields));
}

pub(crate) fn webview_log_line(
    event: &str,
    fields: &BTreeMap<String, serde_json::Value>,
) -> String {
    let mut line = token(&serde_json::Value::from(event));
    for (key, value) in fields.iter().take(MAX_FIELDS) {
        line.push_str(&format!(
            " {}={}",
            token(&serde_json::Value::from(key.as_str())),
            token(value)
        ));
    }
    if fields.len() > MAX_FIELDS {
        line.push_str(&format!(" dropped_fields={}", fields.len() - MAX_FIELDS));
    }
    line
}

/// A number, or a string of identifier characters; anything else is `invalid`.
fn token(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text)
            if !text.is_empty()
                && text.chars().count() <= MAX_TOKEN_CHARS
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.')) =>
        {
            text.clone()
        }
        _ => "invalid".to_string(),
    }
}

#[cfg(test)]
#[path = "desktop_webview_log_tests.rs"]
mod tests;
