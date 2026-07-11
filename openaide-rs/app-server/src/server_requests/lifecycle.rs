use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId, TaskId};
use openaide_app_server_protocol::snapshot::{PendingRequestScope, PendingRequestSnapshot};

use crate::client_lifecycle::{AppServerTime, Delivery};

use super::broker::ServerRequestBroker;
use super::records::{
    can_deliver_to, mark_responder_eligible, mark_responder_stale, record_matches_responder,
    PendingRecord, RequestStatus,
};
use super::types::{RequestLifecycleOutcome, ResponderScope, ServerRequestDelivery};

impl ServerRequestBroker {
    pub fn observe_client_initialized_or_reattached(
        &mut self,
        delivery: Delivery,
        scopes: &[ResponderScope],
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.observe_responder_available(delivery, scopes, now)
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
        outcomes.extend(self.interrupt_orphaned_interactive_requests());
        outcomes
    }

    pub fn observe_subscription_added(
        &mut self,
        delivery: Delivery,
        task_id: TaskId,
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        let mut scopes = self
            .available_responders
            .get(&delivery.client_instance_id)
            .map(|responder| responder.scopes.clone())
            .unwrap_or_else(|| vec![ResponderScope::Client(delivery.client_instance_id.clone())]);
        let task_scope = ResponderScope::Task(task_id);
        if !scopes.contains(&task_scope) {
            scopes.push(task_scope);
        }
        self.observe_responder_available(delivery, &scopes, now)
    }

    pub fn observe_subscription_removed(
        &mut self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        _now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        let scopes = if let Some(responder) = self.available_responders.get_mut(client_instance_id)
        {
            responder.scopes.retain(
                |scope| !matches!(scope, ResponderScope::Task(current) if current.as_str() == task_id.as_str()),
            );
            responder.scopes.clone()
        } else {
            Vec::new()
        };
        for record in self.pending_task_records_mut(task_id) {
            if !record_matches_responder(record, client_instance_id, &scopes) {
                mark_responder_stale(record, client_instance_id);
            }
        }
        self.interrupt_orphaned_interactive_requests()
    }

    pub fn observe_capability_available(
        &mut self,
        delivery: Delivery,
        scopes: &[ResponderScope],
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.observe_responder_available(delivery, scopes, now)
    }

    pub fn observe_capability_unavailable(
        &mut self,
        client_instance_id: &ClientInstanceId,
        scopes: &[ResponderScope],
        _now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        if let Some(responder) = self.available_responders.get_mut(client_instance_id) {
            responder.delivery.request_capabilities.clear();
        }
        for record in self.records.values_mut() {
            if record.status == RequestStatus::Pending
                && record_matches_responder(record, client_instance_id, scopes)
            {
                mark_responder_stale(record, client_instance_id);
            }
        }
        self.interrupt_orphaned_interactive_requests()
    }

    pub fn observe_responder_available(
        &mut self,
        delivery: Delivery,
        scopes: &[ResponderScope],
        _now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.available_responders.insert(
            delivery.client_instance_id.clone(),
            super::broker::AvailableResponder {
                delivery: delivery.clone(),
                scopes: scopes.to_vec(),
            },
        );
        let request_ids: Vec<RequestId> = self
            .records
            .iter()
            .filter(|&(_request_id, record)| {
                record.status == RequestStatus::Pending
                    && can_deliver_to(record, &delivery.client_instance_id)
                    && delivery.supports_method(&record.method)
                    && record_matches_responder(record, &delivery.client_instance_id, scopes)
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

    fn pending_task_records_mut(&mut self, task_id: &TaskId) -> Vec<&mut PendingRecord> {
        self.records
            .values_mut()
            .filter(|record| {
                record.status == RequestStatus::Pending
                    && matches!(&record.snapshot.scope, PendingRequestScope::Task { task_id: target } if target == task_id)
            })
            .collect()
    }

    /// Interactive ACP requests cannot outlive the last client that can answer them.
    fn interrupt_orphaned_interactive_requests(&mut self) -> Vec<RequestLifecycleOutcome> {
        let orphaned = self
            .records
            .iter()
            .filter_map(|(request_id, record)| {
                let interactive = matches!(
                    record.method.as_str(),
                    openaide_app_server_protocol::server_requests::PERMISSION_REQUEST
                        | openaide_app_server_protocol::server_requests::QUESTION_REQUEST
                );
                (record.status == RequestStatus::Pending
                    && interactive
                    && !self.available_responders.values().any(|responder| {
                        responder.delivery.supports_method(&record.method)
                            && record_matches_responder(
                                record,
                                &responder.delivery.client_instance_id,
                                &responder.scopes,
                            )
                    }))
                .then(|| request_id.clone())
            })
            .collect::<Vec<_>>();

        orphaned
            .into_iter()
            .filter_map(|request_id| {
                let record = self.records.get_mut(&request_id)?;
                record.status = RequestStatus::Interrupted;
                Some(RequestLifecycleOutcome::Interrupted {
                    request_id,
                    scope: record.snapshot.scope.clone(),
                })
            })
            .collect()
    }
}
