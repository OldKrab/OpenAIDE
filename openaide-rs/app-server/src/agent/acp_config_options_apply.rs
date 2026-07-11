use std::collections::{HashMap, HashSet};

use agent_client_protocol::schema::{SessionConfigOption, SetSessionConfigOptionRequest};
use agent_client_protocol::{Agent, ConnectionTo, SessionMessage};
use serde_json::Value;
use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_update_projection::{normalize_config_options, PreparedOptionsProjection};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;

async fn set_prepared_config_option(
    connection: &ConnectionTo<Agent>,
    session_id: String,
    config_id: String,
    value: String,
) -> Result<Vec<SessionConfigOption>, RuntimeError> {
    let response = connection
        .send_request(SetSessionConfigOptionRequest::new(
            session_id,
            config_id,
            value.as_str(),
        ))
        .block_task()
        .await
        .map_err(acp_error)?;
    Ok(response.config_options)
}

pub(super) async fn set_prepared_config_option_after_prior_updates(
    connection: &ConnectionTo<Agent>,
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    config_id: String,
    value: String,
    catalog: &mut ConfigOptionsCatalog,
    context: PreparedOptionsSetContext<'_>,
) -> Result<ConfigOptionsCatalog, RuntimeError> {
    let request = set_prepared_config_option(
        connection,
        active_session.session_id().to_string(),
        config_id,
        value,
    );
    tokio::pin!(request);
    let options_projection = PreparedOptionsProjection::new(context.agent_id);
    loop {
        tokio::select! {
            biased;
            invalidation = context.invalidation_rx.recv() => {
                return Err(RuntimeError::NotReady(
                    invalidation.unwrap_or_else(|| "ACP options session invalidated".to_string()),
                ));
            }
            update = active_session.read_update() => {
                match update {
                    Ok(SessionMessage::SessionMessage(dispatch)) => {
                        options_projection.apply_dispatch(dispatch, catalog).await?;
                    }
                    Ok(SessionMessage::StopReason(_)) => {}
                    Ok(_) => {}
                    Err(error) => return Err(acp_error(error)),
                }
            }
            response = &mut request => {
                return response.map(|options| normalize_config_options(context.agent_id, options));
            }
        }
    }
}

pub(super) struct PreparedOptionsSetContext<'a> {
    pub(super) invalidation_rx: &'a mut tokio_mpsc::UnboundedReceiver<String>,
    pub(super) agent_id: &'a str,
}

pub(super) async fn set_task_config_option_after_prior_updates(
    connection: &ConnectionTo<Agent>,
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    config_id: String,
    value: String,
    catalog: &mut ConfigOptionsCatalog,
    agent_id: &str,
) -> Result<ConfigOptionsCatalog, RuntimeError> {
    let options_projection = PreparedOptionsProjection::new(agent_id);
    let request = set_prepared_config_option(
        connection,
        active_session.session_id().to_string(),
        config_id,
        value,
    );
    tokio::pin!(request);
    loop {
        tokio::select! {
            biased;
            update = active_session.read_update() => {
                match update {
                    Ok(SessionMessage::SessionMessage(dispatch)) => {
                        options_projection.apply_dispatch(dispatch, catalog).await?;
                    }
                    Ok(SessionMessage::StopReason(_)) => {}
                    Ok(_) => {}
                    Err(error) => return Err(acp_error(error)),
                }
            }
            response = &mut request => {
                return response.map(|options| normalize_config_options(agent_id, options));
            }
        }
    }
}

pub(super) async fn apply_config_options(
    agent_id: &str,
    connection: &ConnectionTo<Agent>,
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    initial_options: Vec<SessionConfigOption>,
    selected_options: Option<&Value>,
) -> Result<ConfigOptionsCatalog, RuntimeError> {
    let mut selected = config_selection(selected_options)?;
    let mut catalog = normalize_config_options(agent_id, initial_options);
    if selected.is_empty() {
        return Ok(catalog);
    }
    let initial_ids = catalog
        .options
        .iter()
        .map(|option| option.id.clone())
        .collect::<HashSet<_>>();
    let mut seen_ids = initial_ids.clone();

    loop {
        let mut applied = false;
        for option in catalog.options.clone() {
            seen_ids.insert(option.id.clone());
            let Some(value) = selected.get(&option.id).cloned() else {
                continue;
            };
            if !option.values.iter().any(|candidate| candidate.id == value) {
                return Err(RuntimeError::InvalidParams(format!(
                    "config_options.{}",
                    option.id
                )));
            }
            selected.remove(&option.id);
            if option.current_value != value {
                let next_catalog = set_task_config_option_after_prior_updates(
                    connection,
                    active_session,
                    option.id,
                    value,
                    &mut catalog,
                    agent_id,
                )
                .await?;
                catalog = next_catalog;
            }
            applied = true;
            break;
        }
        if !applied {
            break;
        }
    }

    if selected.keys().any(|id| !seen_ids.contains(id)) {
        let id = selected
            .keys()
            .find(|id| !initial_ids.contains(*id))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        return Err(RuntimeError::InvalidParams(format!("config_options.{id}")));
    }

    Ok(catalog)
}

fn config_selection(value: Option<&Value>) -> Result<HashMap<String, String>, RuntimeError> {
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
        let Some(value) = value.as_str() else {
            return Err(RuntimeError::InvalidParams(format!("config_options.{key}")));
        };
        selected.insert(key.clone(), value.to_string());
    }
    Ok(selected)
}
