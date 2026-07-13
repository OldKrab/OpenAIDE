//! Owns the single in-flight ACP prompt for a Native Session.
//!
//! ACP prompt turns are sequential. The session worker therefore keeps one
//! request and one update projection alive until the Agent returns its response.

use std::sync::Arc;

use crate::agent::acp_schema::CancelNotification;
use agent_client_protocol::Agent;
use tokio::sync::mpsc;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host_capabilities::AcpSessionPromptMap;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::prompt_content::{build_prompt_content_with_policy, PromptContentPolicy};
use crate::agent::{AgentEventSink, AgentPrompt, AgentPromptOutcome, TurnCancellation};
use crate::protocol::errors::RuntimeError;

pub(super) struct ActivePrompt {
    completion_rx: mpsc::UnboundedReceiver<PromptCompletion>,
    // Holding the slot keeps host requests bound to this projection until the prompt exits.
    _projection_slot: CurrentPromptSlot,
    cancellation: TurnCancellation,
    task_id: String,
}

impl ActivePrompt {
    // Prompt startup joins ACP request inputs with two independently owned
    // session registries; keeping those seams explicit avoids another context bag.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start(
        active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
        current_prompts: &AcpSessionPromptMap,
        agent_id: &str,
        content_policy: PromptContentPolicy,
        trace: Option<&AcpTraceSession>,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
        session_projection: Option<&LivePromptProjection>,
    ) -> Result<Self, RuntimeError> {
        let projection_slot =
            CurrentPromptSlot::new(current_prompts, &active_session.session_id().to_string());
        let cancellation = prompt.cancellation.clone();
        let task_id = prompt.task_id.clone();
        let projection = LivePromptProjection::for_prompt(
            agent_id,
            sink,
            cancellation.clone(),
            session_projection,
        );
        projection_slot.activate(projection.clone());
        let (completion_tx, completion_rx) = mpsc::unbounded_channel();
        send_prompt_request(active_session, prompt, content_policy, trace, completion_tx)?;
        Ok(Self {
            completion_rx,
            _projection_slot: projection_slot,
            cancellation,
            task_id,
        })
    }

    pub(super) async fn next_completion(&mut self) -> Option<PromptCompletion> {
        self.completion_rx.recv().await
    }

    pub(super) fn cancellation(&self) -> &TurnCancellation {
        &self.cancellation
    }

    pub(super) fn task_id(&self) -> &str {
        &self.task_id
    }
}

/// Holds the ACP response boundary until its preceding session updates are projected.
pub(super) struct PromptCompletion {
    result: Option<Result<AgentPromptOutcome, RuntimeError>>,
    release: Option<tokio::sync::oneshot::Sender<()>>,
}

impl PromptCompletion {
    pub(super) fn finish(mut self) -> Result<AgentPromptOutcome, RuntimeError> {
        let result = self
            .result
            .take()
            .expect("prompt completion is consumed once");
        self.release_boundary();
        result
    }

    fn release_boundary(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl Drop for PromptCompletion {
    fn drop(&mut self) {
        self.release_boundary();
    }
}

struct CurrentPromptSlot {
    current_prompts: AcpSessionPromptMap,
    session_id: String,
}

impl CurrentPromptSlot {
    fn new(current_prompts: &AcpSessionPromptMap, session_id: &str) -> Self {
        Self {
            current_prompts: current_prompts.clone(),
            session_id: session_id.to_string(),
        }
    }

    fn activate(&self, projection: LivePromptProjection) {
        self.current_prompts
            .lock()
            .expect("ACP active prompt poisoned")
            .insert(self.session_id.clone(), projection);
    }
}

impl Drop for CurrentPromptSlot {
    fn drop(&mut self) {
        self.current_prompts
            .lock()
            .expect("ACP active prompt poisoned")
            .remove(&self.session_id);
    }
}

fn send_prompt_request(
    active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
    prompt: AgentPrompt,
    content_policy: PromptContentPolicy,
    trace: Option<&AcpTraceSession>,
    completion_tx: mpsc::UnboundedSender<PromptCompletion>,
) -> Result<(), RuntimeError> {
    let task_id = prompt.task_id.clone();
    let session_id = active_session.session_id().to_string();
    let content = build_prompt_content_with_policy(prompt.text, prompt.attachments, content_policy)
        .map_err(|error| RuntimeError::InvalidParams(error.to_string()))?;
    let request =
        crate::agent::acp_schema::PromptRequest::new(active_session.session_id().clone(), content);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/prompt.request", &request);
    }
    let result_trace = trace.cloned();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    active_session
        .connection()
        .send_request_to(Agent, request)
        .on_receiving_result(async move |result| {
            let result = match result {
                Ok(response) => {
                    if let Some(trace) = &result_trace {
                        trace.record("agent_to_client", "session/prompt.response", &response);
                    }
                    Ok(prompt_outcome(response.stop_reason))
                }
                Err(error) => Err(acp_error(error)),
            };
            if completion_tx
                .send(PromptCompletion {
                    result: Some(result),
                    release: Some(release_tx),
                })
                .is_err()
            {
                crate::logging::warn(
                    "acp_prompt_completion_receiver_dropped",
                    serde_json::json!({
                        "task_id": task_id,
                        "active_session_id": session_id,
                    }),
                );
                return Ok(());
            }
            // Holding the callback keeps later wire messages out of the session queue while
            // the worker projects every update that preceded this prompt response.
            let _ = release_rx.await;
            Ok(())
        })
        .map_err(acp_error)
}

/// Sends steering on the same ACP method while deliberately discarding its
/// eventual response. Session updates continue through the permanent listener.
pub(super) fn send_steering_prompt_request(
    active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
    prompt: AgentPrompt,
    content_policy: PromptContentPolicy,
    trace: Option<&AcpTraceSession>,
) -> Result<(), RuntimeError> {
    let task_id = prompt.task_id.clone();
    let session_id = active_session.session_id().to_string();
    let content = build_prompt_content_with_policy(prompt.text, prompt.attachments, content_policy)
        .map_err(|error| RuntimeError::InvalidParams(error.to_string()))?;
    let request =
        crate::agent::acp_schema::PromptRequest::new(active_session.session_id().clone(), content);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/prompt.request", &request);
    }
    let result_trace = trace.cloned();
    active_session
        .connection()
        .send_request_to(Agent, request)
        .on_receiving_result(async move |result| {
            match result {
                Ok(response) => {
                    if let Some(trace) = &result_trace {
                        trace.record("agent_to_client", "session/prompt.response", &response);
                    }
                    crate::logging::info(
                        "acp_steering_prompt_result",
                        serde_json::json!({
                            "task_id": task_id,
                            "active_session_id": session_id,
                            "result": "stop_reason",
                        }),
                    );
                }
                Err(error) => {
                    crate::logging::warn(
                        "acp_steering_prompt_result",
                        serde_json::json!({
                            "task_id": task_id,
                            "active_session_id": session_id,
                            "result": "error",
                            "error": error.to_string(),
                        }),
                    );
                }
            }
            Ok(())
        })
        .map_err(acp_error)
}

fn prompt_outcome(stop_reason: crate::agent::acp_schema::StopReason) -> AgentPromptOutcome {
    use crate::agent::acp_schema::StopReason;

    match stop_reason {
        StopReason::EndTurn => AgentPromptOutcome::EndTurn,
        StopReason::MaxTokens => AgentPromptOutcome::MaxTokens,
        StopReason::MaxTurnRequests => AgentPromptOutcome::MaxTurnRequests,
        StopReason::Refusal => AgentPromptOutcome::Refusal,
        StopReason::Cancelled => AgentPromptOutcome::Cancelled,
        other => AgentPromptOutcome::Other(
            serde_json::to_value(other)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
        ),
    }
}

pub(super) async fn cancel_active_prompt(
    active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
    trace: Option<&AcpTraceSession>,
) {
    let notification = CancelNotification::new(active_session.session_id().clone());
    if let Some(trace) = trace {
        trace.record(
            "client_to_agent",
            "session/cancel.notification",
            &notification,
        );
    }
    let _ = active_session.connection().send_notification(notification);
}
