use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId, TaskId};
use openaide_app_server_protocol::snapshot::{PendingRequestScope, PendingRequestSnapshot};

use crate::client_lifecycle::{AppServerTime, Delivery};

use super::ServerRequestRuntime;
use crate::server_requests::{RequestLifecycleOutcome, ResponderScope, ServerRequestDelivery};

impl ServerRequestRuntime {
    pub fn interrupt_request(
        &self,
        request_id: &RequestId,
        now: AppServerTime,
    ) -> Option<PendingRequestScope> {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        let scope = inner.broker.interrupt_request(request_id, now);
        super::remove_permission_waiter(&mut inner, request_id);
        inner.question_waiters.remove(request_id);
        scope
    }

    pub fn observe_client_initialized_or_reattached(
        &self,
        delivery: Delivery,
        scopes: &[ResponderScope],
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.inner
            .lock()
            .expect("server request runtime poisoned")
            .broker
            .observe_client_initialized_or_reattached(delivery, scopes, now)
    }

    pub fn observe_transport_unavailable(
        &self,
        client_instance_id: &ClientInstanceId,
        now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.observe_responder_loss(|inner| {
            inner
                .broker
                .observe_transport_unavailable(client_instance_id, now)
        })
    }

    pub fn observe_client_expired(
        &self,
        client_instance_id: &ClientInstanceId,
        now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.observe_responder_loss(|inner| {
            inner.broker.observe_client_expired(client_instance_id, now)
        })
    }

    pub fn observe_responder_available(
        &self,
        delivery: Delivery,
        scopes: &[ResponderScope],
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.inner
            .lock()
            .expect("server request runtime poisoned")
            .broker
            .observe_responder_available(delivery, scopes, now)
    }

    pub fn observe_subscription_added(
        &self,
        delivery: Delivery,
        task_id: TaskId,
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.inner
            .lock()
            .expect("server request runtime poisoned")
            .broker
            .observe_subscription_added(delivery, task_id, now)
    }

    pub fn observe_subscription_removed(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        now: AppServerTime,
    ) -> Vec<RequestLifecycleOutcome> {
        self.observe_responder_loss(|inner| {
            inner
                .broker
                .observe_subscription_removed(client_instance_id, task_id, now)
        })
    }

    pub fn pending_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
    ) -> Vec<PendingRequestSnapshot> {
        self.inner
            .lock()
            .expect("server request runtime poisoned")
            .broker
            .pending_for_client(client_instance_id)
    }

    pub fn pending_for_task(&self, task_id: &TaskId) -> Vec<PendingRequestSnapshot> {
        self.inner
            .lock()
            .expect("server request runtime poisoned")
            .broker
            .pending_for_task(task_id)
    }

    pub fn pending_count(&self) -> usize {
        self.inner
            .lock()
            .expect("server request runtime poisoned")
            .broker
            .pending_count()
    }

    fn observe_responder_loss(
        &self,
        observe: impl FnOnce(&mut super::ServerRequestRuntimeInner) -> Vec<RequestLifecycleOutcome>,
    ) -> Vec<RequestLifecycleOutcome> {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        let outcomes = observe(&mut inner);
        for outcome in &outcomes {
            let RequestLifecycleOutcome::Interrupted { request_id, .. } = outcome;
            super::remove_permission_waiter(&mut inner, request_id);
            inner.question_waiters.remove(request_id);
        }
        if !outcomes.is_empty() {
            self.changed.notify_all();
        }
        outcomes
    }
}
