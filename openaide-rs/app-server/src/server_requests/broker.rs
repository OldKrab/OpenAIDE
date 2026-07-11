use std::collections::{HashMap, HashSet};

use openaide_app_server_protocol::envelopes::ServerRequestEnvelope;
use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId};
use openaide_app_server_protocol::server_requests::{
    PermissionRequestParams, QuestionRequestParams, PERMISSION_REQUEST, QUESTION_REQUEST,
};
use openaide_app_server_protocol::snapshot::{
    PendingRequestKind, PendingRequestScope, PendingRequestSnapshot,
};

use crate::client_lifecycle::{AppServerTime, Delivery};

use super::records::{
    can_deliver_to, mark_responder_delivered, mark_responder_eligible, record_matches_responder,
    PendingRecord, RequestStatus, ResponderState,
};
use super::types::{
    OpenRequestOutcome, RequestUnavailableReason, ResponderScope, ResponseOutcome,
    ServerRequestAnswer, ServerRequestDelivery, ServerRequestDraft,
};

#[derive(Debug, Clone)]
pub struct ServerRequestBroker {
    pub(super) next_request_id: u64,
    pub(super) records: HashMap<RequestId, PendingRecord>,
    pub(super) available_responders: HashMap<ClientInstanceId, AvailableResponder>,
}

#[derive(Debug, Clone)]
pub(super) struct AvailableResponder {
    pub(super) delivery: Delivery,
    pub(super) scopes: Vec<ResponderScope>,
}

impl ServerRequestBroker {
    pub fn new() -> Self {
        Self {
            next_request_id: 1,
            records: HashMap::new(),
            available_responders: HashMap::new(),
        }
    }

    pub fn open(
        &mut self,
        draft: ServerRequestDraft,
        eligible_deliveries: Vec<Delivery>,
        _now: AppServerTime,
    ) -> OpenRequestOutcome {
        let Some(kind) = classify_request_method(&draft.method) else {
            return OpenRequestOutcome::Unavailable {
                reason: RequestUnavailableReason::UnsupportedMethod,
            };
        };

        let mut eligible_deliveries =
            filter_eligible_deliveries(&draft.scope, &draft.method, eligible_deliveries);
        for delivery in &eligible_deliveries {
            let responder = self
                .available_responders
                .entry(delivery.client_instance_id.clone())
                .or_insert_with(|| AvailableResponder {
                    delivery: delivery.clone(),
                    scopes: vec![ResponderScope::Client(delivery.client_instance_id.clone())],
                });
            responder.delivery = delivery.clone();
            if let PendingRequestScope::Task { task_id } = &draft.scope {
                let task_scope = ResponderScope::Task(task_id.clone());
                if !responder.scopes.contains(&task_scope) {
                    responder.scopes.push(task_scope);
                }
            }
        }
        let available_deliveries = self
            .available_responders
            .values()
            .filter(|&responder| {
                responder.delivery.supports_method(&draft.method)
                    && scope_matches_responder(
                        &draft.scope,
                        &draft.method,
                        &responder.delivery.client_instance_id,
                        &responder.scopes,
                    )
                    && !eligible_deliveries.iter().any(|delivery| {
                        delivery.client_instance_id == responder.delivery.client_instance_id
                    })
            })
            .map(|responder| responder.delivery.clone())
            .collect::<Vec<_>>();
        eligible_deliveries.extend(available_deliveries);
        let requires_immediate_responder =
            matches!(draft.method.as_str(), PERMISSION_REQUEST | QUESTION_REQUEST);
        if eligible_deliveries.is_empty()
            && (matches!(draft.scope, PendingRequestScope::Client { .. })
                || requires_immediate_responder)
        {
            return OpenRequestOutcome::Unavailable {
                reason: RequestUnavailableReason::NoEligibleResponder,
            };
        }

        let request_id = self.allocate_request_id();
        let snapshot = PendingRequestSnapshot {
            request_id: request_id.clone(),
            scope: draft.scope,
            kind,
            title: draft.title,
            permission: permission_snapshot(&draft.method, &draft.params),
            question: question_snapshot(&draft.method, &draft.params),
        };
        let record = PendingRecord::new(
            snapshot.clone(),
            draft.method,
            draft.params,
            eligible_deliveries
                .iter()
                .map(|delivery| delivery.client_instance_id.clone()),
        );
        self.records.insert(request_id.clone(), record);

        OpenRequestOutcome::Opened {
            snapshot,
            deliveries: self.mark_and_build_deliveries(&request_id, eligible_deliveries),
        }
    }

    pub fn handle_response(
        &mut self,
        responder: ClientInstanceId,
        request_id: RequestId,
        answer: ServerRequestAnswer,
        _now: AppServerTime,
    ) -> ResponseOutcome {
        self.handle_response_from_scopes(responder, request_id, answer, &[], _now)
    }

    pub fn handle_response_from_scopes(
        &mut self,
        responder: ClientInstanceId,
        request_id: RequestId,
        answer: ServerRequestAnswer,
        responder_scopes: &[ResponderScope],
        _now: AppServerTime,
    ) -> ResponseOutcome {
        let Some(record) = self.records.get_mut(&request_id) else {
            return ResponseOutcome::UnknownRequest { request_id };
        };
        if !record.responders.contains_key(&responder)
            && record_matches_responder(record, &responder, responder_scopes)
        {
            mark_responder_eligible(record, responder.clone());
        }
        match record.responders.get(&responder) {
            Some(ResponderState::Eligible | ResponderState::Delivered) => {}
            Some(ResponderState::Stale) => {
                return ResponseOutcome::StaleRequest {
                    request_id,
                    responder,
                };
            }
            None => {
                return ResponseOutcome::UnauthorizedResponder {
                    request_id,
                    responder,
                };
            }
        }
        match record.status {
            RequestStatus::Resolved => return ResponseOutcome::AlreadyResolved { request_id },
            RequestStatus::Interrupted => return ResponseOutcome::Interrupted { request_id },
            RequestStatus::Pending => {}
        }

        let result = match answer {
            ServerRequestAnswer::Result(value) => value,
            ServerRequestAnswer::Invalid(message) => {
                return ResponseOutcome::InvalidResponse {
                    request_id,
                    responder,
                    message,
                };
            }
        };
        record.status = RequestStatus::Resolved;
        ResponseOutcome::Accepted {
            request_id,
            scope: record.snapshot.scope.clone(),
            responder,
            result,
        }
    }

    pub fn interrupt_request(
        &mut self,
        request_id: &RequestId,
        _now: AppServerTime,
    ) -> Option<PendingRequestScope> {
        let record = self.records.get_mut(request_id)?;
        if record.status != RequestStatus::Pending {
            return None;
        }
        record.status = RequestStatus::Interrupted;
        Some(record.snapshot.scope.clone())
    }

    pub fn resolve_request_without_responder(
        &mut self,
        request_id: &RequestId,
        _now: AppServerTime,
    ) -> Option<PendingRequestScope> {
        let record = self.records.get_mut(request_id)?;
        if record.status != RequestStatus::Pending {
            return None;
        }
        record.status = RequestStatus::Resolved;
        Some(record.snapshot.scope.clone())
    }

    pub(super) fn mark_and_build_deliveries(
        &mut self,
        request_id: &RequestId,
        deliveries: Vec<Delivery>,
    ) -> Vec<ServerRequestDelivery> {
        let Some(record) = self.records.get(request_id) else {
            return Vec::new();
        };
        let method = record.method.clone();
        let params = record.params.clone();
        let scope = record.snapshot.scope.clone();
        let deliveries: Vec<Delivery> = deliveries
            .into_iter()
            .scan(HashSet::new(), |seen, delivery| {
                seen.insert(delivery.client_instance_id.clone())
                    .then_some(delivery)
            })
            .filter(|delivery| can_deliver_to(record, &delivery.client_instance_id))
            .collect();

        if let Some(record) = self.records.get_mut(request_id) {
            for delivery in &deliveries {
                mark_responder_delivered(record, delivery.client_instance_id.clone());
            }
        }

        deliveries
            .into_iter()
            .map(|delivery| ServerRequestDelivery {
                delivery,
                envelope: ServerRequestEnvelope::new(
                    request_id.clone(),
                    scope.clone(),
                    method.clone(),
                    params.clone(),
                ),
            })
            .collect()
    }

    /// Runtime-originated requests are drained by the transport on its next update tick.
    pub(super) fn defer_deliveries(&mut self, deliveries: &[ServerRequestDelivery]) {
        for delivery in deliveries {
            if let Some(record) = self.records.get_mut(&delivery.envelope.request_id) {
                mark_responder_eligible(record, delivery.delivery.client_instance_id.clone());
            }
        }
    }

    fn allocate_request_id(&mut self) -> RequestId {
        let id = RequestId::from(format!("server-request-{}", self.next_request_id));
        self.next_request_id += 1;
        id
    }
}

fn scope_matches_responder(
    scope: &PendingRequestScope,
    method: &str,
    client_instance_id: &ClientInstanceId,
    scopes: &[ResponderScope],
) -> bool {
    let snapshot = PendingRequestSnapshot {
        request_id: RequestId::from("capability-check"),
        scope: scope.clone(),
        kind: PendingRequestKind::Permission,
        title: String::new(),
        permission: None,
        question: None,
    };
    let record = PendingRecord::new(snapshot, method.to_string(), serde_json::Value::Null, []);
    record_matches_responder(&record, client_instance_id, scopes)
}

impl Default for ServerRequestBroker {
    fn default() -> Self {
        Self::new()
    }
}

fn classify_request_method(method: &str) -> Option<PendingRequestKind> {
    if method.starts_with("permission/") {
        Some(PendingRequestKind::Permission)
    } else if method.starts_with("question/") {
        Some(PendingRequestKind::Question)
    } else if method.starts_with("secret/") {
        Some(PendingRequestKind::Secret)
    } else if method.starts_with("shell/") {
        Some(PendingRequestKind::ShellCapability)
    } else {
        None
    }
}

fn question_snapshot(method: &str, params: &serde_json::Value) -> Option<QuestionRequestParams> {
    if method != QUESTION_REQUEST {
        return None;
    }
    serde_json::from_value(params.clone()).ok()
}

fn permission_snapshot(
    method: &str,
    params: &serde_json::Value,
) -> Option<PermissionRequestParams> {
    if method != PERMISSION_REQUEST {
        return None;
    }
    serde_json::from_value(params.clone()).ok()
}

fn filter_eligible_deliveries(
    scope: &PendingRequestScope,
    method: &str,
    deliveries: Vec<Delivery>,
) -> Vec<Delivery> {
    let deliveries = match scope {
        PendingRequestScope::Client { client_instance_id } => deliveries
            .into_iter()
            .filter(|delivery| &delivery.client_instance_id == client_instance_id)
            .collect(),
        PendingRequestScope::Task { .. } => deliveries,
    };
    deliveries
        .into_iter()
        .filter(|delivery| delivery.supports_method(method))
        .collect()
}
