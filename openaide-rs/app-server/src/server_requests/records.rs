use std::collections::HashMap;

use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::snapshot::{PendingRequestScope, PendingRequestSnapshot};
use serde_json::Value;

use super::ResponderScope;

#[derive(Debug, Clone)]
pub(super) struct PendingRecord {
    pub(super) snapshot: PendingRequestSnapshot,
    pub(super) method: String,
    pub(super) params: Value,
    pub(super) responders: HashMap<ClientInstanceId, ResponderState>,
    pub(super) status: RequestStatus,
}

impl PendingRecord {
    pub(super) fn new(
        snapshot: PendingRequestSnapshot,
        method: String,
        params: Value,
        clients: impl IntoIterator<Item = ClientInstanceId>,
    ) -> Self {
        Self {
            snapshot,
            method,
            params,
            responders: clients
                .into_iter()
                .map(|client| (client, ResponderState::Eligible))
                .collect(),
            status: RequestStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResponderState {
    Eligible,
    Delivered,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RequestStatus {
    Pending,
    Resolved,
    Interrupted,
}

pub(super) fn mark_responder_eligible(
    record: &mut PendingRecord,
    client_instance_id: ClientInstanceId,
) {
    record
        .responders
        .insert(client_instance_id, ResponderState::Eligible);
}

pub(super) fn mark_responder_delivered(
    record: &mut PendingRecord,
    client_instance_id: ClientInstanceId,
) {
    record
        .responders
        .insert(client_instance_id, ResponderState::Delivered);
}

pub(super) fn mark_responder_stale(
    record: &mut PendingRecord,
    client_instance_id: &ClientInstanceId,
) {
    if record.responders.contains_key(client_instance_id) {
        record
            .responders
            .insert(client_instance_id.clone(), ResponderState::Stale);
    }
}

pub(super) fn can_deliver_to(
    record: &PendingRecord,
    client_instance_id: &ClientInstanceId,
) -> bool {
    record.responders.get(client_instance_id) != Some(&ResponderState::Delivered)
}

pub(super) fn record_matches_responder(
    record: &PendingRecord,
    client_instance_id: &ClientInstanceId,
    scopes: &[ResponderScope],
) -> bool {
    match &record.snapshot.scope {
        PendingRequestScope::Client {
            client_instance_id: target,
        } => {
            target == client_instance_id
                && scopes
                    .iter()
                    .any(|scope| matches!(scope, ResponderScope::Client(client) if client == target))
        }
        PendingRequestScope::Task { task_id } => scopes
            .iter()
            .any(|scope| matches!(scope, ResponderScope::Task(scope_task_id) if scope_task_id == task_id)),
    }
}
