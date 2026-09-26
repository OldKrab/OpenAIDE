//! Optional ACP v1 steering extension. Its reply acknowledges delivery, never
//! turn completion. Only explicit non-delivery permits the prompt fallback.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use agent_client_protocol::{Agent, ConnectionTo, JsonRpcRequest, JsonRpcResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent::acp_errors::acp_request_error;
use crate::agent::acp_schema::{ContentBlock, InitializeResponse, SessionId};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::attached_native_session::PromptRequestGuard;
use crate::agent::prompt_content::{build_prompt_content_with_policy, PromptContentPolicy};
use crate::agent::AgentPrompt;
use crate::protocol::errors::RuntimeError;

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcRequest)]
#[request(method = "_session/steering", response = SteeringResponse)]
#[serde(rename_all = "camelCase")]
struct SteeringRequest {
    session_id: SessionId,
    prompt: Vec<ContentBlock>,
    #[serde(rename = "_meta")]
    meta: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcResponse)]
struct SteeringResponse {
    outcome: Outcome,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum Outcome {
    Injected,
    PromptRequired,
    StartedNewTurn,
    Failed,
    #[serde(other)]
    Unknown,
}

pub(super) enum SteeringAction {
    Injected,
    ContinuePrompt,
    LegacyPrompt,
    Failed(RuntimeError),
}

pub(super) struct SteeringDelivery {
    pub(super) prompt: AgentPrompt,
    pub(super) request_guard: PromptRequestGuard,
}

type ResponseFuture = Pin<Box<dyn Future<Output = Result<SteeringResponse, RuntimeError>> + Send>>;

struct PendingSteering {
    delivery: SteeringDelivery,
    response: ResponseFuture,
    operation: SteeringOperation,
}

/// Kept by the attachment across turns. A method-not-found response disables
/// the extension for this attachment; transport/provider errors never do.
pub(super) struct SteeringRequests {
    supported: bool,
    pending: VecDeque<PendingSteering>,
}

impl SteeringRequests {
    pub(super) fn new(initialize: &InitializeResponse) -> Self {
        Self {
            supported: initialize
                .meta
                .as_ref()
                .and_then(|meta| meta.get("steering"))
                .and_then(|meta| meta.get("supported"))
                .and_then(serde_json::Value::as_bool)
                == Some(true),
            pending: VecDeque::new(),
        }
    }

    pub(super) fn supported(&self) -> bool {
        self.supported
    }

    pub(super) fn enqueue(
        &mut self,
        connection: &ConnectionTo<Agent>,
        delivery: SteeringDelivery,
        content_policy: PromptContentPolicy,
        trace: Option<&AcpTraceSession>,
    ) -> Result<(), RuntimeError> {
        let request = SteeringRequest {
            session_id: delivery.prompt.session_id.clone().into(),
            prompt: build_prompt_content_with_policy(
                delivery.prompt.text.clone(),
                delivery.prompt.attachments.clone(),
                content_policy,
            )
            .map_err(|error| RuntimeError::InvalidParams(error.to_string()))?,
            // Keep the continuation Host-owned if the native turn wins the race.
            meta: json!({ "steering": { "idleBehavior": "promptRequired" } }),
        };
        let connection = connection.clone();
        let trace = trace.cloned();
        let operation = SteeringOperation::start(&delivery.prompt);
        self.pending.push_back(PendingSteering {
            delivery,
            operation,
            response: Box::pin(async move {
                if let Some(trace) = &trace {
                    trace.record("client_to_agent", "_session/steering.request", &request);
                }
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    connection.send_request_to(Agent, request).block_task(),
                )
                .await
                .map_err(|_| {
                    RuntimeError::OutcomeUnknown("Steering acknowledgment timed out".into())
                })?;
                if let Some(trace) = &trace {
                    match &result {
                        Ok(response) => {
                            trace.record("agent_to_client", "_session/steering.response", response)
                        }
                        Err(error) => {
                            trace.record("agent_to_client", "_session/steering.error", error)
                        }
                    }
                }
                result.map_err(|error| {
                    if error.code == agent_client_protocol::ErrorCode::MethodNotFound {
                        RuntimeError::MethodNotFound("_session/steering".into())
                    } else {
                        acp_request_error(&error)
                    }
                })
            }),
        });
        Ok(())
    }

    /// Serial delivery prevents two late steers from starting rival prompts.
    /// Poll alongside session updates and prompt responses, never across them.
    pub(super) async fn next_response(&mut self) -> (SteeringDelivery, SteeringAction) {
        let Some(pending) = self.pending.front_mut() else {
            return std::future::pending().await;
        };
        let action = if !self.supported {
            SteeringAction::LegacyPrompt
        } else {
            match pending.response.as_mut().await {
                Ok(SteeringResponse {
                    outcome: Outcome::Injected,
                }) => SteeringAction::Injected,
                Ok(SteeringResponse {
                    outcome: Outcome::PromptRequired,
                }) => SteeringAction::ContinuePrompt,
                Err(RuntimeError::MethodNotFound(_)) => {
                    self.supported = false;
                    SteeringAction::LegacyPrompt
                }
                Err(error) => SteeringAction::Failed(error),
                Ok(_) => SteeringAction::Failed(RuntimeError::OutcomeUnknown(
                    "The Agent did not confirm tracked delivery of the message".into(),
                )),
            }
        };
        let mut pending = self
            .pending
            .pop_front()
            .expect("completed steering request");
        pending.operation.finish(&action);
        (pending.delivery, action)
    }

    pub(super) fn abandon(&mut self) {
        self.pending.clear();
    }
}

/// A terminal diagnostic is emitted even when cancellation drops the waiter.
struct SteeringOperation {
    id: String,
    task_id: String,
    session_id: String,
    started: Instant,
    finished: bool,
}

impl SteeringOperation {
    fn start(prompt: &AgentPrompt) -> Self {
        let operation = Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: prompt.task_id.clone(),
            session_id: prompt.session_id.clone(),
            started: Instant::now(),
            finished: false,
        };
        operation.log("started", None);
        operation
    }

    fn finish(&mut self, action: &SteeringAction) {
        let (outcome, error) = match action {
            SteeringAction::Injected => ("injected", None),
            SteeringAction::ContinuePrompt => ("prompt_required", None),
            SteeringAction::LegacyPrompt => ("unsupported", None),
            SteeringAction::Failed(error) => ("failed", Some(error.reason())),
        };
        self.log(outcome, error);
        self.finished = true;
    }

    fn log(&self, outcome: &str, error: Option<&str>) {
        crate::logging::info(
            "acp_steering_delivery",
            json!({
                "operation": "_session/steering", "operation_id": self.id,
                "task_id": self.task_id, "session_id": self.session_id, "attempt": 1,
                "outcome": outcome, "duration_ms": self.started.elapsed().as_millis(),
                "error_kind": error,
            }),
        );
    }
}

impl Drop for SteeringOperation {
    fn drop(&mut self) {
        if !self.finished {
            self.log("cancelled", None);
        }
    }
}
