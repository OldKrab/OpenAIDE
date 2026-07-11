use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;

use super::super::required_prompt_text;

pub(super) fn title_from_prompt(prompt: &str) -> String {
    let title = prompt
        .split_whitespace()
        .take(5)
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() {
        "New task".to_string()
    } else {
        title
    }
}

pub(super) fn title_from_loaded_messages(messages: &[NormalizedMessage]) -> String {
    messages
        .iter()
        .find_map(|message| match message {
            NormalizedMessage::User { text, .. } | NormalizedMessage::AgentText { text, .. } => {
                Some(title_from_prompt(text))
            }
            _ => None,
        })
        .unwrap_or_else(|| "Loaded session".to_string())
}

pub(super) fn required_optional_prompt_text(
    value: Option<String>,
    field: &str,
) -> Result<String, RuntimeError> {
    required_optional_text(value, field)
}

pub(super) fn required_optional_text(
    value: Option<String>,
    field: &str,
) -> Result<String, RuntimeError> {
    value
        .map(|text| required_prompt_text(text, field))
        .unwrap_or_else(|| Err(RuntimeError::InvalidParams(field.to_string())))
}
