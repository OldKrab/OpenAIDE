use crate::agent::acp_schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory as AcpConfigOptionCategory,
    SessionConfigSelectOptions,
};
use crate::protocol::model::{
    ConfigOption, ConfigOptionCategory, ConfigOptionCurrentValue, ConfigOptionKind,
    ConfigOptionValue, ConfigOptionsCatalog, ConfigOptionsStatus,
};
use serde_json::json;

#[cfg(test)]
#[path = "acp_config_projection_tests.rs"]
mod tests;

pub(super) fn normalize_config_options(
    agent_id: &str,
    options: Vec<SessionConfigOption>,
) -> ConfigOptionsCatalog {
    let options = options
        .into_iter()
        .filter_map(normalize_config_option)
        .collect::<Vec<_>>();
    ConfigOptionsCatalog {
        agent_id: agent_id.to_string(),
        status: if options.is_empty() {
            ConfigOptionsStatus::Empty
        } else {
            ConfigOptionsStatus::Ready
        },
        options,
    }
}

fn normalize_config_option(option: SessionConfigOption) -> Option<ConfigOption> {
    let option_id = option.id.to_string();
    let (kind, current_value, values) = match option.kind {
        SessionConfigKind::Select(select) => (
            ConfigOptionKind::Select,
            ConfigOptionCurrentValue::id(select.current_value.to_string()),
            normalize_select_values(select.options),
        ),
        SessionConfigKind::Boolean(boolean) => (
            ConfigOptionKind::Boolean,
            ConfigOptionCurrentValue::boolean(boolean.current_value),
            Vec::new(),
        ),
        _ => {
            crate::logging::warn(
                "acp_config_option_kind_unsupported",
                json!({ "config_id": option_id }),
            );
            return None;
        }
    };
    Some(ConfigOption {
        id: option_id,
        label: option.name,
        description: option.description,
        category: option.category.map(normalize_config_category),
        kind,
        current_value,
        values,
    })
}

fn normalize_select_values(options: SessionConfigSelectOptions) -> Vec<ConfigOptionValue> {
    match options {
        SessionConfigSelectOptions::Ungrouped(values) => values
            .into_iter()
            .map(|value| ConfigOptionValue {
                id: value.value.to_string(),
                label: value.name,
                description: value.description,
                group_id: None,
                group_label: None,
            })
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .into_iter()
            .flat_map(|group| {
                let group_id = group.group.to_string();
                let group_label = group.name;
                group
                    .options
                    .into_iter()
                    .map(move |value| ConfigOptionValue {
                        id: value.value.to_string(),
                        label: value.name,
                        description: value.description,
                        group_id: Some(group_id.clone()),
                        group_label: Some(group_label.clone()),
                    })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_config_category(category: AcpConfigOptionCategory) -> ConfigOptionCategory {
    match category {
        AcpConfigOptionCategory::Mode => ConfigOptionCategory::Mode,
        AcpConfigOptionCategory::Model => ConfigOptionCategory::Model,
        AcpConfigOptionCategory::ThoughtLevel => ConfigOptionCategory::ThoughtLevel,
        AcpConfigOptionCategory::Other(_) => ConfigOptionCategory::Other,
        _ => ConfigOptionCategory::Other,
    }
}
