use std::collections::{HashMap, HashSet};
use std::time::Duration;

use agent_client_protocol::schema::{SessionConfigOption, SetSessionConfigOptionRequest};
use agent_client_protocol::{Agent, ConnectionTo, SessionMessage};
use serde_json::Value;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_update_projection::normalize_config_options;
use crate::agent::{AgentSessionEventSink, ConfigOptionPolicy};
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

pub(super) async fn set_task_config_option_after_prior_updates(
    connection: &ConnectionTo<Agent>,
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    config_id: String,
    value: String,
    agent_id: &str,
) -> Result<OrderedConfigOptionResponse, RuntimeError> {
    let request = SetSessionConfigOptionRequest::new(
        active_session.session_id().clone(),
        config_id,
        value.as_str(),
    );
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let agent_id = agent_id.to_string();
    connection
        .send_request_to(Agent, request)
        .on_receiving_result(move |result| async move {
            let result = result
                .map(|response| normalize_config_options(&agent_id, response.config_options))
                .map_err(acp_error);
            let _ = response_tx.send(result);
            // Holding this callback keeps the ACP dispatch loop on the response
            // boundary, so later notifications cannot enter the session queue.
            let _ = release_rx.await;
            Ok(())
        })
        .map_err(acp_error)?;
    let result = response_rx
        .await
        .map_err(|_| RuntimeError::NotReady("ACP config response channel stopped".to_string()))?;
    let prior_updates = take_queued_session_updates(active_session).await?;
    Ok(OrderedConfigOptionResponse {
        result: Some(result),
        prior_updates,
        release: Some(release_tx),
    })
}

/// Holds the ACP response boundary until its preceding session updates are projected.
pub(super) struct OrderedConfigOptionResponse {
    result: Option<Result<ConfigOptionsCatalog, RuntimeError>>,
    prior_updates: Vec<SessionMessage>,
    release: Option<tokio::sync::oneshot::Sender<()>>,
}

impl OrderedConfigOptionResponse {
    pub(super) fn take_prior_updates(&mut self) -> Vec<SessionMessage> {
        std::mem::take(&mut self.prior_updates)
    }

    /// Projects the response through the same ordered session sink before later
    /// Agent notifications may enter the ACP dispatch loop.
    pub(super) fn finish_with_session_sink(
        mut self,
        session_event_sink: Option<&dyn AgentSessionEventSink>,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        let result = self
            .result
            .take()
            .expect("ordered config response is consumed once");
        if let (Ok(catalog), Some(sink)) = (&result, session_event_sink) {
            sink.config_options_changed(catalog.clone())?;
        }
        self.release_boundary();
        result
    }

    fn release_boundary(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl Drop for OrderedConfigOptionResponse {
    fn drop(&mut self) {
        self.release_boundary();
    }
}

async fn take_queued_session_updates(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
) -> Result<Vec<SessionMessage>, RuntimeError> {
    let mut updates = Vec::new();
    loop {
        // The response callback holds ACP dispatch, so every currently queued
        // message preceded the response and a pending read means the queue is drained.
        let Ok(update) = tokio::time::timeout(Duration::ZERO, active_session.read_update()).await
        else {
            return Ok(updates);
        };
        updates.push(update.map_err(acp_error)?);
    }
}

pub(super) async fn apply_config_options(
    agent_id: &str,
    connection: &ConnectionTo<Agent>,
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    initial_options: Vec<SessionConfigOption>,
    selected_options: Option<&Value>,
    policy: ConfigOptionPolicy,
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
                if policy == ConfigOptionPolicy::ReconcileWithAgentDefaults {
                    selected.remove(&option.id);
                    continue;
                }
                return Err(RuntimeError::InvalidParams(format!(
                    "config_options.{}",
                    option.id
                )));
            }
            selected.remove(&option.id);
            if option.current_value != value {
                let options = set_prepared_config_option(
                    connection,
                    active_session.session_id().to_string(),
                    option.id,
                    value,
                )
                .await?;
                catalog = normalize_config_options(agent_id, options);
            }
            applied = true;
            break;
        }
        if !applied {
            break;
        }
    }

    if selected.keys().any(|id| !seen_ids.contains(id)) {
        if policy == ConfigOptionPolicy::ReconcileWithAgentDefaults {
            return Ok(catalog);
        }
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
