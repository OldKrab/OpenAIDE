use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId, TaskId};
use openaide_app_server_protocol::snapshot::{PendingRequestScope, PendingRequestSnapshot};

use crate::client_lifecycle::{AppServerTime, Delivery};

use super::broker::ServerRequestBroker;
use super::records::{
    can_deliver_to, mark_responder_eligible, mark_responder_stale, record_matches_responder,
    RequestStatus,
};
use super::types::{RequestLifecycleOutcome, ServerRequestDelivery};

impl ServerRequestBroker {
    pub fn observe_client_initialized_or_reattached(
        &mut self,
        delivery: Delivery,
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.observe_responder_available(delivery, now)
    }

    pub fn observe_transport_unavailable(
        &mut self,
        client_instance_id: &ClientInstanceId,
        now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.observe_responder_unavailable(client_instance_id, now)
    }

    pub fn observe_client_expired(
        &mut self,
        client_instance_id: &ClientInstanceId,
        now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.observe_responder_unavailable(client_instance_id, now)
    }

    pub fn observe_responder_unavailable(
        &mut self,
        client_instance_id: &ClientInstanceId,
        _now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.available_responders.remove(client_instance_id);
        self.secret_holders.forget(client_instance_id);
        let mut outcomes = Vec::new();
        for record in self.records.values_mut() {
            if record.status != RequestStatus::Pending {
                continue;
            }
            match &record.snapshot.scope {
                PendingRequestScope::Client {
                    client_instance_id: target,
                } if target == client_instance_id => {
                    record.status = RequestStatus::Interrupted;
                    outcomes.push(RequestLifecycleOutcome::Interrupted {
                        request_id: record.snapshot.request_id.clone(),
                        scope: record.snapshot.scope.clone(),
                    });
                }
                PendingRequestScope::Task { .. } => {
                    mark_responder_stale(record, client_instance_id);
                }
                _ => {}
            }
        }
        outcomes
    }

    pub fn observe_responder_available(
        &mut self,
        delivery: Delivery,
        _now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.available_responders.insert(
            delivery.client_instance_id.clone(),
            super::broker::AvailableResponder {
                delivery: delivery.clone(),
            },
        );
        let request_ids: Vec<RequestId> = self
            .records
            .iter()
            .filter(|&(_request_id, record)| {
                record.status == RequestStatus::Pending
                    && can_deliver_to(record, &delivery.client_instance_id)
                    && !super::types::is_task_state_request(&record.method)
                    && record_matches_responder(record, &delivery.client_instance_id)
            })
            .map(|(request_id, _record)| request_id.clone())
            .collect();

        let mut deliveries = Vec::new();
        for request_id in request_ids {
            if let Some(record) = self.records.get_mut(&request_id) {
                mark_responder_eligible(record, delivery.client_instance_id.clone());
            }
            deliveries.extend(self.mark_and_build_deliveries(&request_id, vec![delivery.clone()]));
        }
        deliveries
    }

    pub fn fail_scope(
        &mut self,
        scope: &PendingRequestScope,
        now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.interrupt_scope(scope, now)
    }

    pub fn observe_task_completed(
        &mut self,
        task_id: &TaskId,
        now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.interrupt_scope(
            &PendingRequestScope::Task {
                task_id: task_id.clone(),
            },
            now,
        )
    }

    pub fn interrupt_scope(
        &mut self,
        scope: &PendingRequestScope,
        _now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        let mut outcomes = Vec::new();
        for record in self.records.values_mut() {
            if record.status == RequestStatus::Pending && &record.snapshot.scope == scope {
                record.status = RequestStatus::Interrupted;
                outcomes.push(RequestLifecycleOutcome::Interrupted {
                    request_id: record.snapshot.request_id.clone(),
                    scope: record.snapshot.scope.clone(),
                });
            }
        }
        outcomes
    }

    pub fn pending_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
    ) -> Vec<PendingRequestSnapshot> {
        self.pending_snapshots(|scope| {
            matches!(scope, PendingRequestScope::Client { client_instance_id: target } if target == client_instance_id)
        })
    }

    pub fn pending_for_task(&self, task_id: &TaskId) -> Vec<PendingRequestSnapshot> {
        self.pending_snapshots(|scope| {
            matches!(scope, PendingRequestScope::Task { task_id: target } if target == task_id)
        })
    }

    pub fn pending_count(&self) -> usize {
        self.records
            .values()
            .filter(|record| record.status == RequestStatus::Pending)
            .count()
    }

    fn pending_snapshots(
        &self,
        matches_scope: impl Fn(&PendingRequestScope) -> bool,
    ) -> Vec<PendingRequestSnapshot> {
        let mut snapshots: Vec<_> = self
            .records
            .values()
            .filter(|record| {
                record.status == RequestStatus::Pending && matches_scope(&record.snapshot.scope)
            })
            .map(|record| record.snapshot.clone())
            .collect();
        snapshots.sort_by(|left, right| left.request_id.cmp(&right.request_id));
        snapshots
    }
}
