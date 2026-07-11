use std::collections::{BTreeMap, HashSet};

use openaide_app_server_protocol::server_requests::{
    QuestionField, QuestionOption, QuestionRequestParams, QuestionStringFormat, QuestionValue,
};
use regex::Regex;

use crate::agent::acp_elicitation_wire::{
    ElicitationSchema, EnumOption, MultiSelectItems, PropertySchema, StringFormat,
};
use crate::protocol::errors::RuntimeError;

mod string_format;
#[cfg(test)]
mod tests;

use string_format::valid_format;

const MAX_FIELDS: usize = 32;
const MAX_CHOICES: usize = 100;
const MAX_TOTAL_BYTES: usize = 256 * 1024;
const MAX_STRING_BYTES: usize = 16 * 1024;
const MAX_PATTERN_BYTES: usize = 1024;

pub(super) fn normalize_form(
    message: String,
    schema: ElicitationSchema,
) -> Result<QuestionRequestParams, RuntimeError> {
    if message.trim().is_empty() {
        return invalid("elicitation message must not be empty");
    }
    enforce_budget(&(&message, &schema), "elicitation request")?;
    if schema.properties.len() > MAX_FIELDS {
        return invalid("elicitation schema exceeds 32 fields");
    }
    let required: HashSet<String> = schema.required.iter().cloned().collect();
    if required.len() != schema.required.len()
        || required
            .iter()
            .any(|key| !schema.properties.contains_key(key))
    {
        return invalid("elicitation required fields must be unique schema properties");
    }

    let fields = schema
        .properties
        .into_iter()
        .map(|(key, property)| normalize_field(key, property, &required))
        .collect::<Result<Vec<_>, _>>()?;
    let form = QuestionRequestParams { message, fields };
    let defaults = defaults(&form);
    for field in &form.fields {
        if let Some(value) = defaults.get(field.key()) {
            validate_value(field, value)?;
        }
    }
    Ok(form)
}

pub(crate) fn validate_product_response(
    form: &QuestionRequestParams,
    response: &openaide_app_server_protocol::server_requests::QuestionRequestResponse,
) -> Result<(), RuntimeError> {
    if let openaide_app_server_protocol::server_requests::QuestionRequestResponse::Submit {
        content,
    } = response
    {
        enforce_budget(content, "elicitation response")?;
        validate_content(form, content)?;
    }
    Ok(())
}

fn normalize_field(
    key: String,
    property: PropertySchema,
    required: &HashSet<String>,
) -> Result<QuestionField, RuntimeError> {
    let is_required = required.contains(&key);
    let fallback = key.clone();
    Ok(match property {
        PropertySchema::String {
            title,
            description,
            min_length,
            max_length,
            pattern,
            format,
            default,
            enum_values,
            one_of,
        } => {
            if enum_values.is_some() && one_of.is_some() {
                return invalid("string field cannot contain both enum and oneOf");
            }
            if let Some(pattern) = &pattern {
                if pattern.len() > MAX_PATTERN_BYTES || Regex::new(pattern).is_err() {
                    return invalid("string field pattern is invalid or exceeds 1 KiB");
                }
            }
            if let Some(options) = enum_values
                .map(plain_options)
                .or_else(|| one_of.map(titled_options))
            {
                validate_options(&options)?;
                QuestionField::SingleSelect {
                    key,
                    title: title.unwrap_or(fallback),
                    description,
                    required: is_required,
                    default,
                    options,
                }
            } else {
                if min_length
                    .zip(max_length)
                    .is_some_and(|(min, max)| min > max)
                {
                    return invalid("string field minimum exceeds maximum");
                }
                QuestionField::String {
                    key,
                    title: title.unwrap_or(fallback),
                    description,
                    required: is_required,
                    default,
                    min_length,
                    max_length,
                    pattern,
                    format: format.map(map_format),
                }
            }
        }
        PropertySchema::Number {
            title,
            description,
            minimum,
            maximum,
            default,
        } => {
            if minimum
                .into_iter()
                .chain(maximum)
                .chain(default)
                .any(|value| !value.is_finite())
                || minimum.zip(maximum).is_some_and(|(min, max)| min > max)
            {
                return invalid("number field range/default is invalid");
            }
            QuestionField::Number {
                key,
                title: title.unwrap_or(fallback),
                description,
                required: is_required,
                default,
                minimum,
                maximum,
            }
        }
        PropertySchema::Integer {
            title,
            description,
            minimum,
            maximum,
            default,
        } => {
            if minimum.zip(maximum).is_some_and(|(min, max)| min > max) {
                return invalid("integer field minimum exceeds maximum");
            }
            QuestionField::Integer {
                key,
                title: title.unwrap_or(fallback),
                description,
                required: is_required,
                default,
                minimum,
                maximum,
            }
        }
        PropertySchema::Boolean {
            title,
            description,
            default,
        } => QuestionField::Boolean {
            key,
            title: title.unwrap_or(fallback),
            description,
            required: is_required,
            default,
        },
        PropertySchema::Array {
            title,
            description,
            min_items,
            max_items,
            items,
            default,
        } => {
            if min_items.zip(max_items).is_some_and(|(min, max)| min > max) {
                return invalid("multi-select minimum exceeds maximum");
            }
            let options = match items {
                MultiSelectItems::Untitled { values, .. } => plain_options(values),
                MultiSelectItems::Titled { options } => titled_options(options),
            };
            validate_options(&options)?;
            QuestionField::MultiSelect {
                key,
                title: title.unwrap_or(fallback),
                description,
                required: is_required,
                default,
                min_items,
                max_items,
                options,
            }
        }
    })
}

fn validate_content(
    form: &QuestionRequestParams,
    content: &BTreeMap<String, QuestionValue>,
) -> Result<(), RuntimeError> {
    if content
        .keys()
        .any(|key| !form.fields.iter().any(|field| field.key() == key))
    {
        return invalid("elicitation response contains an unknown field");
    }
    for field in &form.fields {
        let value = content.get(field.key());
        if value.is_none() && field.required() {
            return invalid("elicitation response is missing a required field");
        }
        if let Some(value) = value {
            validate_value(field, value)?;
        }
    }
    Ok(())
}

fn validate_value(field: &QuestionField, value: &QuestionValue) -> Result<(), RuntimeError> {
    match (field, value) {
        (
            QuestionField::String {
                min_length,
                max_length,
                pattern,
                format,
                ..
            },
            QuestionValue::String(value),
        ) => {
            if value.len() > MAX_STRING_BYTES
                || min_length.is_some_and(|min| value.chars().count() < min as usize)
                || max_length.is_some_and(|max| value.chars().count() > max as usize)
                || pattern.as_ref().is_some_and(|pattern| {
                    !Regex::new(pattern).is_ok_and(|regex| regex.is_match(value))
                })
                || format.is_some_and(|format| !valid_format(format, value))
            {
                return invalid("elicitation string value does not match its schema");
            }
        }
        (QuestionField::SingleSelect { options, .. }, QuestionValue::String(value)) => {
            if !options.iter().any(|option| option.value == *value) {
                return invalid("elicitation selection is not allowed");
            }
        }
        (
            QuestionField::Number {
                minimum, maximum, ..
            },
            QuestionValue::Number(value),
        ) => {
            if !value.is_finite()
                || minimum.is_some_and(|min| *value < min)
                || maximum.is_some_and(|max| *value > max)
            {
                return invalid("elicitation number is outside its range");
            }
        }
        (
            QuestionField::Integer {
                minimum, maximum, ..
            },
            QuestionValue::Integer(value),
        ) => {
            if minimum.is_some_and(|min| *value < min) || maximum.is_some_and(|max| *value > max) {
                return invalid("elicitation integer is outside its range");
            }
        }
        (QuestionField::Boolean { .. }, QuestionValue::Boolean(_)) => {}
        (
            QuestionField::MultiSelect {
                min_items,
                max_items,
                options,
                ..
            },
            QuestionValue::StringArray(values),
        ) => {
            let unique: HashSet<_> = values.iter().collect();
            if unique.len() != values.len()
                || min_items.is_some_and(|min| values.len() < min as usize)
                || max_items.is_some_and(|max| values.len() > max as usize)
                || values
                    .iter()
                    .any(|value| !options.iter().any(|option| option.value == *value))
            {
                return invalid("elicitation multi-selection does not match its schema");
            }
        }
        _ => return invalid("elicitation response value has the wrong type"),
    }
    Ok(())
}

fn defaults(form: &QuestionRequestParams) -> BTreeMap<String, QuestionValue> {
    form.fields
        .iter()
        .filter_map(|field| match field {
            QuestionField::String {
                key,
                default: Some(value),
                ..
            }
            | QuestionField::SingleSelect {
                key,
                default: Some(value),
                ..
            } => Some((key.clone(), QuestionValue::String(value.clone()))),
            QuestionField::Number {
                key,
                default: Some(value),
                ..
            } => Some((key.clone(), QuestionValue::Number(*value))),
            QuestionField::Integer {
                key,
                default: Some(value),
                ..
            } => Some((key.clone(), QuestionValue::Integer(*value))),
            QuestionField::Boolean {
                key,
                default: Some(value),
                ..
            } => Some((key.clone(), QuestionValue::Boolean(*value))),
            QuestionField::MultiSelect { key, default, .. } if !default.is_empty() => {
                Some((key.clone(), QuestionValue::StringArray(default.clone())))
            }
            _ => None,
        })
        .collect()
}

fn validate_options(options: &[QuestionOption]) -> Result<(), RuntimeError> {
    let unique: HashSet<_> = options.iter().map(|option| &option.value).collect();
    if options.is_empty() || options.len() > MAX_CHOICES || unique.len() != options.len() {
        return invalid("elicitation choices must be unique and contain 1 to 100 items");
    }
    Ok(())
}

fn plain_options(values: Vec<String>) -> Vec<QuestionOption> {
    values
        .into_iter()
        .map(|value| QuestionOption {
            label: value.clone(),
            value,
            description: None,
        })
        .collect()
}

fn titled_options(values: Vec<EnumOption>) -> Vec<QuestionOption> {
    values
        .into_iter()
        .map(|option| QuestionOption {
            value: option.value,
            label: option.title,
            description: option.description,
        })
        .collect()
}

fn map_format(format: StringFormat) -> QuestionStringFormat {
    match format {
        StringFormat::Email => QuestionStringFormat::Email,
        StringFormat::Uri => QuestionStringFormat::Uri,
        StringFormat::Date => QuestionStringFormat::Date,
        StringFormat::DateTime => QuestionStringFormat::DateTime,
    }
}

fn enforce_budget(value: &impl serde::Serialize, label: &str) -> Result<(), RuntimeError> {
    if serde_json::to_vec(value)
        .map_err(|error| RuntimeError::Internal(error.to_string()))?
        .len()
        > MAX_TOTAL_BYTES
    {
        return invalid(&format!("{label} exceeds 256 KiB"));
    }
    Ok(())
}

fn invalid<T>(message: &str) -> Result<T, RuntimeError> {
    Err(RuntimeError::InvalidParams(message.to_string()))
}
