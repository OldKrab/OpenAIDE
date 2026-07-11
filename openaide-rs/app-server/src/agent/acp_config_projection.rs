use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory as AcpConfigOptionCategory,
    SessionConfigSelectOptions, SessionNotification, SessionUpdate,
};
use agent_client_protocol::util::MatchDispatch;

use crate::agent::acp_errors::acp_error;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ConfigOption, ConfigOptionCategory, ConfigOptionValue, ConfigOptionsCatalog,
    ConfigOptionsStatus,
};

pub(super) struct PreparedOptionsProjection<'a> {
    agent_id: &'a str,
}

impl<'a> PreparedOptionsProjection<'a> {
    pub(super) fn new(agent_id: &'a str) -> Self {
        Self { agent_id }
    }

    pub(super) fn catalog(&self, options: Vec<SessionConfigOption>) -> ConfigOptionsCatalog {
        normalize_config_options(self.agent_id, options)
    }

    pub(super) async fn apply_dispatch(
        &self,
        dispatch: agent_client_protocol::Dispatch,
        catalog: &mut ConfigOptionsCatalog,
    ) -> Result<(), RuntimeError> {
        match options_session_update_from_dispatch(self.agent_id, dispatch).await? {
            Some(OptionsSessionUpdate::Config(next_catalog)) => {
                *catalog = next_catalog;
            }
            Some(OptionsSessionUpdate::WorkActivity) => {
                return Err(RuntimeError::NotReady(
                    "ACP options session received task activity".to_string(),
                ));
            }
            None => {}
        }
        Ok(())
    }
}

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
    let (current_value, values) = match option.kind {
        SessionConfigKind::Select(select) => (
            select.current_value.to_string(),
            normalize_select_values(select.options),
        ),
        _ => return None,
    };
    Some(ConfigOption {
        id: option.id.to_string(),
        label: option.name,
        description: option.description,
        category: option.category.map(normalize_config_category),
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

#[derive(Debug)]
enum OptionsSessionUpdate {
    Config(ConfigOptionsCatalog),
    WorkActivity,
}

async fn options_session_update_from_dispatch(
    agent_id: &str,
    dispatch: agent_client_protocol::Dispatch,
) -> Result<Option<OptionsSessionUpdate>, RuntimeError> {
    let replacement: Arc<Mutex<Option<OptionsSessionUpdate>>> = Arc::default();
    let replacement_sink = replacement.clone();
    MatchDispatch::new(dispatch)
        .if_notification(async move |notification: SessionNotification| {
            *replacement_sink
                .lock()
                .expect("ACP options session update lock poisoned") =
                options_session_update(agent_id, notification.update);
            Ok(())
        })
        .await
        .otherwise_ignore()
        .map_err(acp_error)?;
    let result = replacement
        .lock()
        .expect("ACP options session update lock poisoned")
        .take();
    Ok(result)
}

fn options_session_update(agent_id: &str, update: SessionUpdate) -> Option<OptionsSessionUpdate> {
    match update {
        SessionUpdate::ConfigOptionUpdate(update) => Some(OptionsSessionUpdate::Config(
            normalize_config_options(agent_id, update.config_options),
        )),
        SessionUpdate::UserMessageChunk(_)
        | SessionUpdate::AgentMessageChunk(_)
        | SessionUpdate::AgentThoughtChunk(_)
        | SessionUpdate::ToolCall(_)
        | SessionUpdate::ToolCallUpdate(_)
        | SessionUpdate::Plan(_) => Some(OptionsSessionUpdate::WorkActivity),
        _ => None,
    }
}
