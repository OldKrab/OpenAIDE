//! Owns the current ACP prompt set for a Native Session.
//!
//! One primary request owns Task lifecycle while additional prompt requests may
//! steer the same work. The first current `end_turn` settles the shared prompt set.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::agent::acp_schema::CancelNotification;
use agent_client_protocol::Agent;
use tokio::sync::mpsc;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host_capabilities::AcpSessionPromptMap;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::{LivePromptProjection, PrecedingUpdateDrain};
use crate::agent::attached_native_session::PromptRequestGuard;
use crate::agent::events::{AgentEvent, AgentTurnUsage};
use crate::agent::prompt_content::{build_prompt_content_with_policy, PromptContentPolicy};
use crate::agent::{AgentEventSink, AgentPrompt, AgentPromptOutcome, TurnCancellation};
use crate::protocol::errors::RuntimeError;

pub(super) struct ActivePrompt {
    completion_tx: mpsc::UnboundedSender<PromptCompletion>,
    completion_rx: mpsc::UnboundedReceiver<PromptCompletion>,
    settlement: Arc<PromptSettlementState>,
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
        request_guard: PromptRequestGuard,
        preceding_update_drain: Option<PrecedingUpdateDrain>,
    ) -> Result<Self, RuntimeError> {
        let projection_slot =
            CurrentPromptSlot::new(current_prompts, &active_session.session_id().to_string());
        let cancellation = prompt.cancellation.clone();
        let task_id = prompt.task_id.clone();
        let settlement = request_guard.settlement_state();
        let mut projection = LivePromptProjection::for_prompt(
            agent_id,
            sink.clone(),
            cancellation.clone(),
            session_projection,
        );
        if let Some(drain) = preceding_update_drain {
            projection = projection.with_preceding_update_drain(drain);
        }
        projection_slot.activate(projection.clone());
        let (completion_tx, completion_rx) = mpsc::unbounded_channel();
        send_prompt_request(
            active_session,
            prompt,
            content_policy,
            trace,
            completion_tx.clone(),
            settlement.clone(),
            sink,
            request_guard,
        )?;
        Ok(Self {
            completion_tx,
            completion_rx,
            settlement,
            _projection_slot: projection_slot,
            cancellation,
            task_id,
        })
    }

    pub(super) async fn next_completion(&mut self) -> Option<PromptCompletion> {
        self.completion_rx.recv().await
    }

    pub(super) fn steering_settlement(&self) -> PromptSettlement {
        PromptSettlement {
            completion_tx: self.completion_tx.clone(),
            settlement: self.settlement.clone(),
        }
    }

    pub(super) fn mark_settled(&self, kind: PromptSettlementKind) {
        self.settlement.settle(kind);
    }

    pub(super) fn mark_cancel_requested(&self) {
        self.settlement.request_cancellation();
    }

    pub(super) fn cancellation(&self) -> &TurnCancellation {
        &self.cancellation
    }

    pub(super) fn task_id(&self) -> &str {
        &self.task_id
    }
}

/// Lets an `end_turn` steering response settle the same lifecycle as the primary prompt.
pub(super) struct PromptSettlement {
    completion_tx: mpsc::UnboundedSender<PromptCompletion>,
    settlement: Arc<PromptSettlementState>,
}

#[derive(Clone, Copy)]
pub(super) enum PromptSettlementKind {
    PromptResponse = 1,
    RunnerExit = 2,
}

impl PromptSettlementKind {
    fn label(self) -> &'static str {
        match self {
            Self::PromptResponse => "prompt_response",
            Self::RunnerExit => "runner_exit",
        }
    }
}

#[derive(Default)]
pub(super) struct PromptSettlementState {
    kind: AtomicU8,
    // Some Agents answer the primary request with `cancelled` when steering
    // replaces it. That response must not settle the shared prompt set.
    steering_accepted: AtomicBool,
    // Session cancellation is delivered independently of the primary prompt's
    // cancellation token, so it must override the steering handoff.
    cancel_requested: AtomicBool,
}

impl PromptSettlementState {
    fn settle(&self, kind: PromptSettlementKind) {
        let _ = self
            .kind
            .compare_exchange(0, kind as u8, Ordering::AcqRel, Ordering::Acquire);
    }

    pub(super) fn accept_steering(&self) {
        self.steering_accepted.store(true, Ordering::Release);
    }

    pub(super) fn request_cancellation(&self) {
        self.cancel_requested.store(true, Ordering::Release);
    }

    pub(super) fn kind(&self) -> Option<PromptSettlementKind> {
        match self.kind.load(Ordering::Acquire) {
            value if value == PromptSettlementKind::PromptResponse as u8 => {
                Some(PromptSettlementKind::PromptResponse)
            }
            value if value == PromptSettlementKind::RunnerExit as u8 => {
                Some(PromptSettlementKind::RunnerExit)
            }
            _ => None,
        }
    }

    fn steering_accepted(&self) -> bool {
        self.steering_accepted.load(Ordering::Acquire)
    }

    fn cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Acquire)
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

// Prompt dispatch keeps response projection and generation ownership explicit at this boundary.
#[allow(clippy::too_many_arguments)]
fn send_prompt_request(
    active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
    prompt: AgentPrompt,
    content_policy: PromptContentPolicy,
    trace: Option<&AcpTraceSession>,
    completion_tx: mpsc::UnboundedSender<PromptCompletion>,
    settlement: Arc<PromptSettlementState>,
    sink: Arc<dyn AgentEventSink>,
    request_guard: PromptRequestGuard,
) -> Result<(), RuntimeError> {
    let task_id = prompt.task_id.clone();
    let session_id = active_session.session_id().to_string();
    let cancellation = prompt.cancellation.clone();
    let content = build_prompt_content_with_policy(prompt.text, prompt.attachments, content_policy)
        .map_err(|error| RuntimeError::InvalidParams(error.to_string()))?;
    let request =
        crate::agent::acp_schema::PromptRequest::new(active_session.session_id().clone(), content);
    if let Some(trace) = trace {
        trace.record("client_to_agent", "session/prompt.request", &request);
    }
    let result_trace = trace.cloned();
    let prompt_started_at = Instant::now();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    active_session
        .connection()
        .send_request_to(Agent, request)
        .on_receiving_result(async move |result| {
            let _request_guard = request_guard;
            let result = match result {
                Ok(response) => {
                    if let Some(trace) = &result_trace {
                        trace.record("agent_to_client", "session/prompt.response", &response);
                    }
                    if let Some(usage) = response.usage {
                        sink.emit(AgentEvent::TurnUsage(AgentTurnUsage {
                            total_tokens: usage.total_tokens,
                            input_tokens: usage.input_tokens,
                            output_tokens: usage.output_tokens,
                            reasoning_tokens: usage.thought_tokens,
                            cached_read_tokens: usage.cached_read_tokens,
                            cached_write_tokens: usage.cached_write_tokens,
                        }))
                        .map(|()| prompt_outcome(response.stop_reason))
                    } else {
                        Ok(prompt_outcome(response.stop_reason))
                    }
                }
                Err(error) => Err(acp_error(error)),
            };
            let superseded_by_steering = matches!(&result, Ok(AgentPromptOutcome::Cancelled))
                && settlement.steering_accepted()
                && !cancellation.is_cancelled()
                && !settlement.cancel_requested();
            if settlement.kind().is_some() || superseded_by_steering {
                crate::logging::info(
                    "acp_prompt_result_stale",
                    serde_json::json!({
                        "task_id": task_id,
                        "active_session_id": session_id,
                        "prompt_kind": "primary",
                        "settlement_kind": settlement.kind().map(PromptSettlementKind::label),
                        "stale_reason": if superseded_by_steering {
                            "superseded_by_steering"
                        } else {
                            "already_settled"
                        },
                        "result_status": if result.is_ok() { "stop_reason" } else { "error" },
                        "duration_ms": prompt_started_at.elapsed().as_millis(),
                    }),
                );
                return Ok(());
            }
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

/// Sends steering on the same ACP method. A current `end_turn` joins the primary
/// completion channel; other outcomes remain prompt-local diagnostics.
pub(super) fn send_steering_prompt_request(
    active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
    prompt: AgentPrompt,
    content_policy: PromptContentPolicy,
    trace: Option<&AcpTraceSession>,
    settlement: Option<PromptSettlement>,
    usage_sink: Option<Arc<dyn AgentEventSink>>,
    request_guard: PromptRequestGuard,
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
            let _request_guard = request_guard;
            match result {
                Ok(response) => {
                    if let Some(trace) = &result_trace {
                        trace.record("agent_to_client", "session/prompt.response", &response);
                    }
                    let usage_error = response.usage.and_then(|usage| {
                        usage_sink.as_ref().and_then(|sink| {
                            sink.emit(AgentEvent::TurnUsage(AgentTurnUsage {
                                total_tokens: usage.total_tokens,
                                input_tokens: usage.input_tokens,
                                output_tokens: usage.output_tokens,
                                reasoning_tokens: usage.thought_tokens,
                                cached_read_tokens: usage.cached_read_tokens,
                                cached_write_tokens: usage.cached_write_tokens,
                            }))
                            .err()
                        })
                    });
                    let outcome = prompt_outcome(response.stop_reason);
                    crate::logging::info(
                        "acp_steering_prompt_result",
                        serde_json::json!({
                            "task_id": task_id,
                            "active_session_id": session_id,
                            "result": "stop_reason",
                            "stop_reason": runtime_outcome_name(&outcome),
                        }),
                    );
                    if outcome != AgentPromptOutcome::EndTurn {
                        return Ok(());
                    }
                    let Some(settlement) = settlement else {
                        return Ok(());
                    };
                    if let Some(settlement_kind) = settlement.settlement.kind() {
                        crate::logging::info(
                            "acp_prompt_result_stale",
                            serde_json::json!({
                                "task_id": task_id,
                                "active_session_id": session_id,
                                "prompt_kind": "steering",
                                "settlement_kind": settlement_kind.label(),
                                "result_status": "stop_reason",
                            }),
                        );
                        return Ok(());
                    }
                    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
                    if settlement
                        .completion_tx
                        .send(PromptCompletion {
                            result: Some(match usage_error {
                                Some(error) => Err(error),
                                None => Ok(outcome),
                            }),
                            release: Some(release_tx),
                        })
                        .is_ok()
                    {
                        // Preserve the response boundary until all earlier session updates
                        // have been projected before the Task publishes Idle.
                        let _ = release_rx.await;
                    }
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

fn runtime_outcome_name(outcome: &AgentPromptOutcome) -> &'static str {
    match outcome {
        AgentPromptOutcome::EndTurn => "end_turn",
        AgentPromptOutcome::MaxTokens => "max_tokens",
        AgentPromptOutcome::MaxTurnRequests => "max_turn_requests",
        AgentPromptOutcome::Refusal => "refusal",
        AgentPromptOutcome::Cancelled => "cancelled",
        AgentPromptOutcome::Other(_) => "other",
    }
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
) -> Result<(), RuntimeError> {
    let dispatch_started = Instant::now();
    let notification = CancelNotification::new(active_session.session_id().clone());
    if let Some(trace) = trace {
        trace.record_value(
            "runtime",
            "session/cancel.dispatch_started",
            serde_json::json!({ "sessionId": active_session.session_id() }),
        );
        trace.record(
            "client_to_agent",
            "session/cancel.notification",
            &notification,
        );
    }
    // ACP notifications have no response. Completion here only proves that the
    // connection accepted the notification for transport, not that the Agent acted on it.
    let result = active_session
        .connection()
        .send_notification(notification)
        .map_err(acp_error);
    if let Some(trace) = trace {
        let elapsed_ms = dispatch_started.elapsed().as_millis();
        match &result {
            Ok(()) => trace.record_value(
                "runtime",
                "session/cancel.dispatch_completed",
                serde_json::json!({
                    "sessionId": active_session.session_id(),
                    "elapsed_ms": elapsed_ms,
                }),
            ),
            Err(error) => trace.record_value(
                "runtime",
                "session/cancel.dispatch_failed",
                serde_json::json!({
                    "sessionId": active_session.session_id(),
                    "elapsed_ms": elapsed_ms,
                    "error_code": error.code(),
                    "error_kind": error.reason(),
                }),
            ),
        }
    }
    result
}
