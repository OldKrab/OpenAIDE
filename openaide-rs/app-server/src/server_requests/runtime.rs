use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId, TaskId};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use serde_json::json;

mod lifecycle;
mod permissions;
mod questions;
mod waitable;

use crate::agent::events::{AgentPermissionOutcome, AgentPermissionRequest};
use crate::agent::TurnCancellation;
use crate::client_lifecycle::{AppServerTime, Delivery};
use crate::logging;
use crate::protocol::errors::RuntimeError;

use super::{
    OpenRequestOutcome, ResponderScope, ResponseOutcome, ServerRequestAnswer, ServerRequestBroker,
    ServerRequestDraft,
};
use permissions::{
    option_id_from_result, permission_params, set_permission_outcome, PermissionResponse,
    PermissionWaiter,
};

#[derive(Clone, Default)]
pub struct ServerRequestRuntime {
    inner: Arc<Mutex<ServerRequestRuntimeInner>>,
    changed: Arc<Condvar>,
}

#[derive(Default)]
struct ServerRequestRuntimeInner {
    broker: ServerRequestBroker,
    permission_waiters: HashMap<RequestId, PermissionWaiter>,
    permission_by_agent_request: HashMap<String, RequestId>,
    waitable_requests: HashMap<RequestId, waitable::WaitableRequest>,
    question_waiters: HashMap<RequestId, questions::QuestionWaiter>,
}

impl ServerRequestRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(
        &self,
        draft: ServerRequestDraft,
        deliveries: Vec<Delivery>,
        now: AppServerTime,
    ) -> OpenRequestOutcome {
        self.inner
            .lock()
            .expect("server request runtime poisoned")
            .broker
            .open(draft, deliveries, now)
    }

    pub fn open_permission_request(
        &self,
        task_id: &str,
        request: &AgentPermissionRequest,
        deliveries: Vec<Delivery>,
        now: AppServerTime,
    ) -> Result<RequestId, RuntimeError> {
        let draft = ServerRequestDraft {
            scope: PendingRequestScope::Task {
                task_id: TaskId::from(task_id.to_string()),
            },
            method: "permission/request".to_string(),
            title: request.title.clone(),
            params: permission_params(request),
        };
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        let opened = inner.broker.open(draft, deliveries, now);
        let OpenRequestOutcome::Opened { snapshot, .. } = opened else {
            return Err(RuntimeError::NotReady(
                "permission request is unavailable".to_string(),
            ));
        };
        let request_id = snapshot.request_id;
        logging::info(
            "server_permission_request_opened",
            json!({
                "task_id": task_id,
                "agent_request_id": request.request_id.as_str(),
                "server_request_id": request_id.as_str(),
                "tool_call_id": request.tool_call.tool_call_id.as_str(),
                "tool_kind": request.tool_call.kind.as_deref(),
                "option_count": request.options.len(),
            }),
        );
        inner
            .permission_by_agent_request
            .insert(request.request_id.clone(), request_id.clone());
        inner
            .permission_waiters
            .insert(request_id.clone(), PermissionWaiter::new(request));
        Ok(request_id)
    }

    pub fn wait_permission_response(
        &self,
        request_id: &RequestId,
        cancellation: &TurnCancellation,
    ) -> Result<PermissionResponse, RuntimeError> {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        loop {
            if cancellation.is_cancelled() {
                inner.broker.interrupt_request(request_id, AppServerTime(0));
                remove_permission_waiter(&mut inner, request_id);
                logging::warn(
                    "server_permission_wait_cancelled",
                    json!({
                        "server_request_id": request_id.as_str(),
                    }),
                );
                return Ok(PermissionResponse {
                    outcome: AgentPermissionOutcome::Cancelled,
                    decision: None,
                });
            }
            if let Some(waiter) = inner.permission_waiters.get_mut(request_id) {
                if let Some(outcome) = waiter.outcome.take() {
                    remove_permission_waiter(&mut inner, request_id);
                    logging::info(
                        "server_permission_wait_released",
                        json!({
                            "server_request_id": request_id.as_str(),
                            "outcome": agent_permission_outcome_name(&outcome.outcome),
                            "has_decision": outcome.decision.is_some(),
                        }),
                    );
                    return Ok(outcome);
                }
            } else {
                logging::warn(
                    "server_permission_waiter_missing",
                    json!({
                        "server_request_id": request_id.as_str(),
                    }),
                );
                return Ok(PermissionResponse {
                    outcome: AgentPermissionOutcome::Cancelled,
                    decision: None,
                });
            }
            let (next_inner, _) = self
                .changed
                .wait_timeout(inner, std::time::Duration::from_millis(50))
                .expect("server request runtime poisoned");
            inner = next_inner;
        }
    }

    pub fn route_agent_permission_response<T>(
        &self,
        agent_request_id: &str,
        option_id: String,
        commit: impl FnOnce(bool) -> Result<T, RuntimeError>,
    ) -> Result<T, RuntimeError> {
        let request_id = self.reserve_legacy_permission_response(agent_request_id, &option_id)?;
        let result = commit(request_id.is_some());
        if result.is_ok() {
            if let Some(request_id) = request_id {
                self.resolve_legacy_permission_request(&request_id);
            }
            self.changed.notify_all();
        } else if let Some(request_id) = request_id {
            self.clear_permission_outcome(&request_id);
        }
        result
    }

    pub fn handle_response(
        &self,
        responder: ClientInstanceId,
        request_id: RequestId,
        answer: ServerRequestAnswer,
        now: AppServerTime,
    ) -> ResponseOutcome {
        self.handle_response_from_scopes(responder, request_id, answer, &[], now)
    }

    pub fn handle_response_from_scopes(
        &self,
        responder: ClientInstanceId,
        request_id: RequestId,
        answer: ServerRequestAnswer,
        responder_scopes: &[ResponderScope],
        now: AppServerTime,
    ) -> ResponseOutcome {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        let answer = normalized_answer_for_waiter(&inner, &request_id, answer);
        let answer = questions::normalize_question_answer(&inner, &request_id, answer);
        let outcome = inner.broker.handle_response_from_scopes(
            responder,
            request_id.clone(),
            answer,
            responder_scopes,
            now,
        );
        logging::info(
            "server_request_response_handled",
            json!({
                "server_request_id": request_id.as_str(),
                "outcome": response_outcome_name(&outcome),
                "responder_scope_count": responder_scopes.len(),
            }),
        );
        if let ResponseOutcome::Accepted { result, .. } = &outcome {
            if let Some(option_id) = option_id_from_result(result) {
                let accepted =
                    set_permission_outcome(&mut inner.permission_waiters, &request_id, option_id);
                logging::info(
                    "server_permission_outcome_set",
                    json!({
                        "server_request_id": request_id.as_str(),
                        "accepted_by_waiter": accepted,
                    }),
                );
                if !accepted {
                    inner.broker.interrupt_request(&request_id, now);
                }
                self.changed.notify_all();
            }
            if let Some(waiter) = inner.waitable_requests.get_mut(&request_id) {
                waiter.result = Some(result.clone());
                self.changed.notify_all();
            }
            if questions::set_question_result(&mut inner, &request_id, result) {
                self.changed.notify_all();
            }
        }
        outcome
    }

    fn reserve_legacy_permission_response(
        &self,
        agent_request_id: &str,
        option_id: &str,
    ) -> Result<Option<RequestId>, RuntimeError> {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        let request_id = inner
            .permission_by_agent_request
            .get(agent_request_id)
            .cloned();
        let Some(request_id) = request_id else {
            return Ok(None);
        };
        if inner
            .permission_waiters
            .get(&request_id)
            .is_some_and(|waiter| waiter.outcome.is_some())
        {
            return Err(RuntimeError::InvalidParams(
                "permission already answered".to_string(),
            ));
        }
        if !set_permission_outcome(
            &mut inner.permission_waiters,
            &request_id,
            option_id.to_string(),
        ) {
            return Err(RuntimeError::InvalidParams("option_id".to_string()));
        }
        Ok(Some(request_id))
    }

    fn clear_permission_outcome(&self, request_id: &RequestId) {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        if let Some(waiter) = inner.permission_waiters.get_mut(request_id) {
            waiter.outcome = None;
        }
    }

    fn resolve_legacy_permission_request(&self, request_id: &RequestId) {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        inner
            .broker
            .resolve_request_without_responder(request_id, AppServerTime(0));
    }
}

fn response_outcome_name(outcome: &ResponseOutcome) -> &'static str {
    match outcome {
        ResponseOutcome::Accepted { .. } => "accepted",
        ResponseOutcome::InvalidResponse { .. } => "invalid_response",
        ResponseOutcome::AlreadyResolved { .. } => "already_resolved",
        ResponseOutcome::UnknownRequest { .. } => "unknown_request",
        ResponseOutcome::UnauthorizedResponder { .. } => "unauthorized_responder",
        ResponseOutcome::StaleRequest { .. } => "stale_request",
        ResponseOutcome::Interrupted { .. } => "interrupted",
    }
}

fn agent_permission_outcome_name(outcome: &AgentPermissionOutcome) -> &'static str {
    match outcome {
        AgentPermissionOutcome::Selected { .. } => "selected",
        AgentPermissionOutcome::Cancelled => "cancelled",
    }
}

fn normalized_answer_for_waiter(
    inner: &ServerRequestRuntimeInner,
    request_id: &RequestId,
    answer: ServerRequestAnswer,
) -> ServerRequestAnswer {
    if !inner.permission_waiters.contains_key(request_id) {
        return answer;
    }
    match answer {
        ServerRequestAnswer::Result(result) => match option_id_from_result(&result) {
            Some(option_id) if waiter_allows_option(inner, request_id, &option_id) => {
                ServerRequestAnswer::Result(result)
            }
            Some(_) => ServerRequestAnswer::Invalid("unknown permission option".to_string()),
            None => ServerRequestAnswer::Invalid("missing permission optionId".to_string()),
        },
        invalid => invalid,
    }
}

fn waiter_allows_option(
    inner: &ServerRequestRuntimeInner,
    request_id: &RequestId,
    option_id: &str,
) -> bool {
    inner
        .permission_waiters
        .get(request_id)
        .is_some_and(|waiter| waiter.allows_option(option_id))
}

fn remove_permission_waiter(inner: &mut ServerRequestRuntimeInner, request_id: &RequestId) {
    if let Some(waiter) = inner.permission_waiters.remove(request_id) {
        inner
            .permission_by_agent_request
            .remove(&waiter.agent_request_id);
    }
}

#[cfg(test)]
mod tests;
