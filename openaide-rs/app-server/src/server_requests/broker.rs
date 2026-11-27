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
    OpenRequestOutcome, RequestUnavailableReason, ResponseOutcome, ServerRequestAnswer,
    ServerRequestDelivery, ServerRequestDraft,
};

#[derive(Debug, Clone)]
pub struct ServerRequestBroker {
    pub(super) next_request_id: u64,
    pub(super) records: HashMap<RequestId, PendingRecord>,
    pub(super) available_responders: HashMap<ClientInstanceId, AvailableResponder>,
    /// Which connected clients hold which secure-storage keys (ADR-0086, `secret/read`).
    pub(super) secret_holders: super::secret_holders::SecretHolders,
}

#[derive(Debug, Clone)]
pub(super) struct AvailableResponder {
    pub(super) delivery: Delivery,
}

impl ServerRequestBroker {
    pub(crate) fn is_auth_terminal_exchange(&self, request_id: &RequestId) -> bool {
        self.records.get(request_id).is_some_and(|record| {
            record.method == openaide_app_server_protocol::server_requests::SHELL_AUTH_TERMINAL
        })
    }
    pub(crate) fn forget_auth_terminal_exchange(&mut self, request_id: &RequestId) {
        if self.records.get(request_id).is_some_and(|record| {
            record.method == openaide_app_server_protocol::server_requests::SHELL_AUTH_TERMINAL
        }) {
            self.records.remove(request_id);
        }
    }

    pub fn new() -> Self {
        Self {
            next_request_id: 1,
            records: HashMap::new(),
            available_responders: HashMap::new(),
            secret_holders: Default::default(),
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

        let eligible_deliveries =
            filter_eligible_deliveries(&draft.scope, &draft.method, eligible_deliveries);
        for delivery in &eligible_deliveries {
            let responder = self
                .available_responders
                .entry(delivery.client_instance_id.clone())
                .or_insert_with(|| AvailableResponder {
                    delivery: delivery.clone(),
                });
            responder.delivery = delivery.clone();
        }
        if eligible_deliveries.is_empty()
            && matches!(draft.scope, PendingRequestScope::Client { .. })
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
            created_at: Some(crate::time::now_string()),
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
        let Some(record) = self.records.get_mut(&request_id) else {
            return ResponseOutcome::UnknownRequest { request_id };
        };
        if !record.responders.contains_key(&responder)
            && record_matches_responder(record, &responder)
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
