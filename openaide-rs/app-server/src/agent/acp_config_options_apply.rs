use crate::agent::acp_schema::{SessionConfigOptionValue, SetSessionConfigOptionRequest};
use agent_client_protocol::{Agent, ConnectionTo, SessionMessage};

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_response_boundary::take_preceding_session_updates;
use crate::agent::acp_update_projection::normalize_config_options;
use crate::agent::AgentSessionEventSink;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionCurrentValue, ConfigOptionsCatalog};

#[cfg(test)]
#[path = "acp_config_options_apply_tests.rs"]
mod tests;

pub(super) async fn set_task_config_option_after_prior_updates(
    connection: &ConnectionTo<Agent>,
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
    config_id: String,
    value: ConfigOptionCurrentValue,
    agent_id: &str,
) -> Result<OrderedConfigOptionResponse, RuntimeError> {
    let request = SetSessionConfigOptionRequest::new(
        active_session.session_id().clone(),
        config_id,
        acp_config_value(value),
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
    let prior_updates = take_preceding_session_updates(active_session).await?;
    Ok(OrderedConfigOptionResponse {
        result: Some(result),
        prior_updates,
        release: Some(release_tx),
    })
}

fn acp_config_value(value: ConfigOptionCurrentValue) -> SessionConfigOptionValue {
    match value {
        ConfigOptionCurrentValue::Id { value } => SessionConfigOptionValue::value_id(value),
        ConfigOptionCurrentValue::Boolean { value } => SessionConfigOptionValue::boolean(value),
    }
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
