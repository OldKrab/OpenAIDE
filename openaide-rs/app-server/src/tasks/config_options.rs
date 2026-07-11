use std::collections::HashMap;

use serde_json::Value;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;

pub(crate) fn selected_config_options(
    value: Option<&Value>,
) -> Result<HashMap<String, String>, RuntimeError> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    if value.is_null() {
        return Ok(HashMap::new());
    }
    let object = value
        .as_object()
        .ok_or_else(|| RuntimeError::InvalidParams("config_options".to_string()))?;
    let mut selected = HashMap::new();
    for (key, value) in object {
        if key.trim().is_empty() {
            return Err(RuntimeError::InvalidParams("config_options".to_string()));
        }
        let Some(value) = value.as_str() else {
            return Err(RuntimeError::InvalidParams(format!("config_options.{key}")));
        };
        selected.insert(key.clone(), value.to_string());
    }
    Ok(selected)
}

pub(crate) fn validate_config_selection(
    catalog: &ConfigOptionsCatalog,
    selected: &HashMap<String, String>,
) -> Result<(), RuntimeError> {
    if selected.is_empty() {
        return Ok(());
    }
    for (config_id, value) in selected {
        let Some(option) = catalog
            .options
            .iter()
            .find(|option| option.id == *config_id)
        else {
            return Err(RuntimeError::InvalidParams(format!(
                "config_options.{config_id}"
            )));
        };
        if !option.values.iter().any(|candidate| candidate.id == *value) {
            return Err(RuntimeError::InvalidParams(format!(
                "config_options.{config_id}"
            )));
        }
    }
    Ok(())
}
