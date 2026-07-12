//! Dispatches same-session prompts immediately and tracks their responses independently.
//! Steering remains part of the active logical turn, so all concurrent prompt requests share
//! one update projection and its source-message continuity state.

use std::collections::HashMap;
use std::sync::{mpsc, Arc};

use agent_client_protocol::schema::CancelNotification;
use agent_client_protocol::Agent;
use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_errors::acp_error;
use crate::agent::acp_host_capabilities::AcpSessionPromptMap;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::prompt_content::{build_prompt_content_with_policy, PromptContentPolicy};
use crate::agent::{AgentEventSink, AgentPrompt, TurnCancellation};
use crate::protocol::errors::RuntimeError;

type PromptToken = u64;

pub(super) struct ConcurrentPrompts {
    waiters: HashMap<PromptToken, PromptWaiter>,
    completion_tx: tokio_mpsc::UnboundedSender<PromptCompletion>,
    completion_rx: tokio_mpsc::UnboundedReceiver<PromptCompletion>,
    next_token: PromptToken,
    primary_token: PromptToken,
    primary_result: Option<Result<(), RuntimeError>>,
    cancelled: bool,
    // Holding the slot keeps the active projection registered until the prompt group exits.
    _projection_slot: CurrentPromptSlot,
    projection: LivePromptProjection,
    cancellation: TurnCancellation,
    task_id: String,
}

impl ConcurrentPrompts {
    #[allow(clippy::too_many_arguments)]
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
        let (completion_tx, completion_rx) = tokio_mpsc::unbounded_channel();
        let primary_token = 0;
        send_prompt_request(
            active_session,
            prompt,
            content_policy,
            trace,
            primary_token,
            completion_tx.clone(),
        )?;
        Ok(Self {
            waiters: HashMap::from([(primary_token, PromptWaiter { done_tx: None })]),
            completion_tx,
            completion_rx,
            next_token: 1,
            primary_token,
            primary_result: None,
            cancelled: false,
            _projection_slot: projection_slot,
            projection,
            cancellation,
            task_id,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch(
        &mut self,
        active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
        _agent_id: &str,
        content_policy: PromptContentPolicy,
        trace: Option<&AcpTraceSession>,
        prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
        done_tx: mpsc::Sender<Result<(), RuntimeError>>,
    ) {
        if self.cancelled || prompt.cancellation.is_cancelled() {
            let _ = done_tx.send(Ok(()));
            return;
        }
        if let Err(error) = self.projection.prepare_for_steering() {
            let _ = done_tx.send(Err(error));
            return;
        }
        let token = self.next_token;
        self.next_token += 1;
        self.waiters.insert(
            token,
            PromptWaiter {
                done_tx: Some(done_tx.clone()),
            },
        );
        if let Err(error) = send_prompt_request(
            active_session,
            prompt,
            content_policy,
            trace,
            token,
            self.completion_tx.clone(),
        ) {
            self.waiters.remove(&token);
            let _ = done_tx.send(Err(error));
        }
    }

    pub(super) async fn next_completion(&mut self) -> Option<PromptCompletion> {
        self.completion_rx.recv().await
    }

    pub(super) fn complete(
        &mut self,
        completion: PromptCompletion,
    ) -> Option<Result<(), RuntimeError>> {
        let mut waiter = self.waiters.remove(&completion.token)?;
        if completion.result.is_ok() {
            // Some Agents coalesce same-session steering requests into the active
            // turn and emit one terminal response for the whole logical turn.
            if let Some(done_tx) = waiter.done_tx.take() {
                let _ = done_tx.send(Ok(()));
            }
            for (_, mut pending) in self.waiters.drain() {
                if let Some(done_tx) = pending.done_tx.take() {
                    let _ = done_tx.send(Ok(()));
                }
            }
            self.primary_result = None;
            return Some(Ok(()));
        }
        if completion.token == self.primary_token {
            self.primary_result = Some(completion.result);
        } else if let Some(done_tx) = waiter.done_tx.take() {
            let _ = done_tx.send(completion.result);
        }
        if self.waiters.is_empty() {
            self.primary_result.take()
        } else {
            None
        }
    }

    pub(super) fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub(super) fn finish(&mut self, message: &str) {
        self.cancelled = true;
        for (_, mut waiter) in self.waiters.drain() {
            if let Some(done_tx) = waiter.done_tx.take() {
                let _ = done_tx.send(Err(RuntimeError::NotReady(message.to_string())));
            }
        }
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

pub(super) struct PromptCompletion {
    token: PromptToken,
    result: Result<(), RuntimeError>,
}

impl PromptCompletion {
    pub(super) fn is_ok(&self) -> bool {
        self.result.is_ok()
    }
}

struct PromptWaiter {
    done_tx: Option<mpsc::Sender<Result<(), RuntimeError>>>,
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

#[allow(clippy::too_many_arguments)]
fn send_prompt_request(
    active_session: &agent_client_protocol::ActiveSession<'static, Agent>,
    prompt: AgentPrompt,
    content_policy: PromptContentPolicy,
    trace: Option<&AcpTraceSession>,
    token: PromptToken,
    completion_tx: tokio_mpsc::UnboundedSender<PromptCompletion>,
) -> Result<(), RuntimeError> {
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
            let _ = completion_tx.send(PromptCompletion { token, result });
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
