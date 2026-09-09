use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::agent::acp_schema::{SessionConfigOptionValue, SetSessionConfigOptionRequest};
use agent_client_protocol::Agent;
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_update_projection::normalize_config_options;
use crate::agent::attached_native_session::AcpSessionConfigCommand;
use crate::agent::AgentSessionEventSink;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionCurrentValue, ConfigOptionsCatalog};

#[cfg(test)]
#[path = "acp_config_options_apply_tests.rs"]
mod tests;

const CONFIG_OPTION_TIMEOUT: Duration = Duration::from_secs(60);
type ConfigReply = mpsc::Sender<Result<ConfigOptionsCatalog, RuntimeError>>;

/// Session-owned configuration requests survive prompt completion. Responses are
/// events in the same loop as prompt results, updates, and cancellation: waiting
/// inside a command handler deadlocks when an earlier prompt response holds ACP's
/// shared dispatch boundary until that loop consumes it.
pub(super) struct SessionConfigRequests {
    response_tx: tokio_mpsc::UnboundedSender<OrderedConfigOptionResponse>,
    response_rx: tokio_mpsc::UnboundedReceiver<OrderedConfigOptionResponse>,
    pending: Option<PendingConfigRequest>,
    outstanding: HashSet<u64>,
    next_id: u64,
}

struct PendingConfigRequest {
    id: u64,
    deadline: tokio::time::Instant,
    operation_id: String,
    reply_tx: ConfigReply,
}

impl SessionConfigRequests {
    pub(super) fn new() -> Self {
        let (response_tx, response_rx) = tokio_mpsc::unbounded_channel();
        Self {
            response_tx,
            response_rx,
            pending: None,
            outstanding: HashSet::new(),
            next_id: 0,
        }
    }

    /// Detaching a session resolves callers and releases held response callbacks.
    pub(super) fn abandon(&mut self) {
        *self = Self::new();
    }

    pub(super) fn can_dispatch(&self) -> bool {
        self.pending.is_none()
    }

    /// Load awaits another ACP response outside the event loop. Even after the
    /// caller times out, an old config callback still needs that loop to release
    /// its response boundary before Load can safely replace this attachment.
    pub(super) fn can_replace_attachment(&self) -> bool {
        self.outstanding.is_empty()
    }

    pub(super) fn dispatch(
        &mut self,
        active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
        command: AcpSessionConfigCommand,
    ) {
        let AcpSessionConfigCommand::SetConfigOption {
            agent_id,
            session_id,
            config_id,
            value,
            operation_id,
            queued_at,
            reply_tx,
        } = command;
        crate::logging::info(
            "acp_config_option_command_received",
            serde_json::json!({
                "session_id": session_id,
                "operation_id": operation_id,
                "queue_wait_ms": queued_at.elapsed().as_millis(),
            }),
        );
        let request = SetSessionConfigOptionRequest::new(
            active_session.session_id().clone(),
            config_id,
            acp_config_value(value),
        );
        self.next_id += 1;
        let request_id = self.next_id;
        let deadline = tokio::time::Instant::from_std(queued_at + CONFIG_OPTION_TIMEOUT);
        // Queued requests have the same deadline as the caller; never dispatch a
        // stale mutation after its user-visible timeout.
        if deadline <= tokio::time::Instant::now() {
            let _ = reply_tx.send(Err(config_timeout()));
            return;
        }
        self.pending = Some(PendingConfigRequest {
            id: request_id,
            deadline,
            operation_id: operation_id.clone(),
            reply_tx: reply_tx.clone(),
        });
        self.outstanding.insert(request_id);
        let response_tx = self.response_tx.clone();
        let request_started_at = Instant::now();
        crate::logging::info(
            "acp_config_option_request_dispatched",
            serde_json::json!({
                "session_id": session_id,
                "operation_id": operation_id,
            }),
        );
        let result = active_session
            .connection()
            .send_request_to(Agent, request)
            .on_receiving_result(move |result| async move {
                crate::logging::info(
                    "acp_config_option_response_received",
                    serde_json::json!({
                        "operation_id": operation_id,
                        "agent_response_ms": request_started_at.elapsed().as_millis(),
                        "result_status": if result.is_ok() { "ok" } else { "error" },
                    }),
                );
                let result = result
                    .map(|response| normalize_config_options(&agent_id, response.config_options))
                    .map_err(acp_error);
                let (release_tx, release_rx) = oneshot::channel();
                let _ = response_tx.send(OrderedConfigOptionResponse {
                    request_id,
                    session_id,
                    operation_id,
                    result: Some(result),
                    reply_tx: Some(reply_tx),
                    release: Some(release_tx),
                });
                // The consumer drains preceding session updates and publishes this
                // catalog before releasing later messages on the shared connection.
                let _ = release_rx.await;
                Ok(())
            });
        if let Err(error) = result {
            self.outstanding.remove(&request_id);
            if let Some(pending) = self.pending.take() {
                let _ = pending.reply_tx.send(Err(acp_error(error)));
            }
        }
    }

    /// A timeout releases command admission, while a late response still enters
    /// the same ordered stream. Its request identity cannot clear a newer wait.
    pub(super) async fn next_response(&mut self) -> Option<OrderedConfigOptionResponse> {
        let deadline = self.pending.as_ref().map(|pending| pending.deadline);
        tokio::select! {
            response = self.response_rx.recv() => {
                if let Some(response) = &response {
                    self.outstanding.remove(&response.request_id);
                }
                if response.as_ref().is_some_and(|response| {
                    self.pending.as_ref().is_some_and(|pending| pending.id == response.request_id)
                }) {
                    self.pending.take();
                }
                response
            }
            () = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(pending) = self.pending.take() {
                    crate::logging::warn("acp_config_option_request_timed_out", serde_json::json!({
                        "operation_id": pending.operation_id,
                        "outcome": "timeout",
                    }));
                    let _ = pending.reply_tx.send(Err(config_timeout()));
                }
                None
            }
        }
    }
}

impl Drop for SessionConfigRequests {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.take() {
            let _ = pending.reply_tx.send(Err(session_stopped()));
        }
    }
}

fn acp_config_value(value: ConfigOptionCurrentValue) -> SessionConfigOptionValue {
    match value {
        ConfigOptionCurrentValue::Id { value } => SessionConfigOptionValue::value_id(value),
        ConfigOptionCurrentValue::Boolean { value } => SessionConfigOptionValue::boolean(value),
    }
}

/// Holds the ACP response boundary until its preceding session updates are projected.
pub(super) struct OrderedConfigOptionResponse {
    request_id: u64,
    session_id: String,
    operation_id: String,
    result: Option<Result<ConfigOptionsCatalog, RuntimeError>>,
    reply_tx: Option<ConfigReply>,
    release: Option<oneshot::Sender<()>>,
}

impl OrderedConfigOptionResponse {
    /// The caller must drain preceding session updates before publishing this result.
    pub(super) fn finish_with_session_sink(
        mut self,
        session_event_sink: Option<&dyn AgentSessionEventSink>,
    ) -> Option<ConfigOptionsCatalog> {
        let result = self
            .result
            .take()
            .expect("ordered config response is consumed once")
            .and_then(|catalog| {
                if let Some(sink) = session_event_sink {
                    sink.config_options_changed(catalog.clone())?;
                }
                Ok(catalog)
            });
        crate::logging::info(
            "acp_config_option_catalog_published",
            serde_json::json!({
                "session_id": self.session_id,
                "operation_id": self.operation_id,
                "result_status": if result.is_ok() { "ok" } else { "error" },
            }),
        );
        let catalog = result.as_ref().ok().cloned();
        if let Some(reply_tx) = self.reply_tx.take() {
            let _ = reply_tx.send(result);
        }
        catalog
    }
}

impl Drop for OrderedConfigOptionResponse {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(reply_tx) = self.reply_tx.take() {
            let _ = reply_tx.send(Err(session_stopped()));
        }
    }
}

fn config_timeout() -> RuntimeError {
    RuntimeError::NotReady("ACP config update timed out".to_string())
}

fn session_stopped() -> RuntimeError {
    RuntimeError::NotReady("Native Session attachment stopped".to_string())
}
