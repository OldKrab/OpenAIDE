//! Owns the single in-flight ACP prompt for a Native Session.
//!
//! ACP prompt turns are sequential. The session worker therefore keeps one
//! request and one update projection alive until the Agent returns its response.

use std::sync::Arc;

use agent_client_protocol::schema::CancelNotification;
use agent_client_protocol::Agent;
use tokio::sync::mpsc;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host_capabilities::AcpSessionPromptMap;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::prompt_content::{build_prompt_content_with_policy, PromptContentPolicy};
use crate::agent::{AgentEventSink, AgentPrompt, TurnCancellation};
use crate::protocol::errors::RuntimeError;

pub(super) struct ActivePrompt {
    completion_rx: mpsc::UnboundedReceiver<Result<(), RuntimeError>>,
    // Holding the slot keeps host requests bound to this projection until the prompt exits.
    _projection_slot: CurrentPromptSlot,
    projection: LivePromptProjection,
    cancellation: TurnCancellation,
    task_id: String,
}

impl ActivePrompt {
    pub(super) fn start(
        active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
        current_prompts: &AcpSessionPromptMap,
        agent_id: &str,
        content_policy: PromptContentPolicy,
        trace: Option<&AcpTraceSession>,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<Self, RuntimeError> {
        let projection_slot =
            CurrentPromptSlot::new(current_prompts, &active_session.session_id().to_string());
        let cancellation = prompt.cancellation.clone();
        let task_id = prompt.task_id.clone();
        let projection = LivePromptProjection::new(agent_id, sink, cancellation.clone());
        projection_slot.activate(projection.clone());
        let (completion_tx, completion_rx) = mpsc::unbounded_channel();
        send_prompt_request(active_session, prompt, content_policy, trace, completion_tx)?;
        Ok(Self {
            completion_rx,
            _projection_slot: projection_slot,
            projection,
            cancellation,
            task_id,
        })
    }

    pub(super) async fn next_completion(&mut self) -> Option<Result<(), RuntimeError>> {
        self.completion_rx.recv().await
    }

    pub(super) fn cancellation(&self) -> &TurnCancellation {
        &self.cancellation
    }

    pub(super) fn projection(&self) -> LivePromptProjection {
        self.projection.clone()
    }

    pub(super) fn task_id(&self) -> &str {
        &self.task_id
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
    completion_tx: mpsc::UnboundedSender<Result<(), RuntimeError>>,
) -> Result<(), RuntimeError> {
    let task_id = prompt.task_id.clone();
    let session_id = active_session.session_id().to_string();
    let content = build_prompt_content_with_policy(prompt.text, prompt.attachments, content_policy)
        .map_err(|error| RuntimeError::InvalidParams(error.to_string()))?;
    let request = agent_client_protocol::schema::PromptRequest::new(
        active_session.session_id().clone(),
        content,
    );
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/prompt.request", &request);
    }
    let result_trace = trace.cloned();
    active_session
        .connection()
        .send_request_to(Agent, request)
        .on_receiving_result(async move |result| {
            let result = match result {
                Ok(response) => {
                    if let Some(trace) = &result_trace {
                        trace.record("agent_to_client", "session/prompt.response", &response);
                    }
                    Ok(())
                }
                Err(error) => Err(acp_error(error)),
            };
            if completion_tx.send(result).is_err() {
                crate::logging::warn(
                    "acp_prompt_completion_receiver_dropped",
                    serde_json::json!({
                        "task_id": task_id,
                        "active_session_id": session_id,
                    }),
                );
            }
            Ok(())
        })
        .map_err(acp_error)
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
