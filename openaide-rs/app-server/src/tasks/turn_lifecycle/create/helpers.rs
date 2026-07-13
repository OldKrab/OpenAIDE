use super::super::required_prompt_text;
use crate::protocol::errors::RuntimeError;

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
