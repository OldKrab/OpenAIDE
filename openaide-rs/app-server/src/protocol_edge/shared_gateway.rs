use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::ids::ClientInstanceId;
#[cfg(test)]
use openaide_app_server_protocol::ids::TaskId;

use crate::app_lifecycle::ShutdownCompletion;
use crate::client_lifecycle::{AppServerTime, ClientExpiryOutcome, ConnectionId};
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestDelivery;
#[cfg(test)]
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use crate::task_events::TaskUpdate;

use super::{
    AppServerProbeFacts, GatewayEventDelivery, GatewayOutcome, IdleShutdownDecision,
    InboundProtocolMessage, RpcGateway,
};

#[derive(Clone)]
pub struct SharedRpcGateway {
    gateway: Arc<Mutex<RpcGateway>>,
    native_catalog_refresh_in_flight: Arc<AtomicBool>,
}

impl SharedRpcGateway {
    pub fn new(gateway: RpcGateway) -> Self {
        Self {
            gateway: Arc::new(Mutex::new(gateway)),
            native_catalog_refresh_in_flight: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Starts at most one slow Native Session catalog refresh without holding the gateway lock.
    pub fn request_native_session_catalog_refresh(&self) {
        if self
            .native_catalog_refresh_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let workflow = self
            .gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .agent_list_sessions
            .clone();
        let in_flight = self.native_catalog_refresh_in_flight.clone();
        std::thread::spawn(move || {
            if let Err(error) = workflow.refresh_native_session_catalogs() {
                crate::logging::warn(
                    "native_session_catalog_refresh_failed",
                    serde_json::json!({ "error": error.message }),
                );
            }
            in_flight.store(false, Ordering::Release);
        });
    }

    #[cfg(test)]
    pub(crate) fn native_catalog_refresh_is_running_for_test(&self) -> bool {
        self.native_catalog_refresh_in_flight
            .load(Ordering::Acquire)
    }

    pub fn handle_inbound(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
    ) -> GatewayOutcome {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .handle_inbound(connection_id, message, now)
    }

    pub fn probe_facts(&self) -> AppServerProbeFacts {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .probe_facts()
    }

    pub fn connection_is_initialized(&self, connection_id: &ConnectionId) -> bool {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .client_hub
            .context_for_connection(connection_id)
            .is_some()
    }

    /// Keeps a client live while its event stream is still accepting writes.
    pub fn observe_event_stream_activity(
        &self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> bool {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .observe_event_stream_activity(connection_id, now)
    }

    pub fn publish_committed_task_update(
        &self,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .publish_task_update(update, now)
    }

    pub fn publish_committed_task_update_for_connection(
        &self,
        connection_id: &ConnectionId,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> (Vec<GatewayEventDelivery>, Vec<ServerRequestDelivery>) {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .publish_committed_task_update_for_connection(connection_id, update, now)
    }

    pub fn drain_server_requests_for_connection(
        &self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .drain_server_requests_for_connection(connection_id, now)
    }

    pub fn drain_event_deliveries_for_connection(
        &self,
        connection_id: &ConnectionId,
    ) -> Vec<GatewayEventDelivery> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .drain_event_deliveries_for_connection(connection_id)
    }

    pub fn expire_client_after_reconnect_grace(
        &self,
        client_instance_id: &ClientInstanceId,
        now: AppServerTime,
    ) -> ClientExpiryOutcome {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .expire_client_after_reconnect_grace(client_instance_id, now)
    }

    pub fn expire_inactive_clients(&self, now: AppServerTime) -> Vec<ClientExpiryOutcome> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .expire_inactive_clients(now)
    }

    pub fn shutdown(&self) -> Result<ShutdownCompletion, RuntimeError> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .shutdown()
    }

    pub(crate) fn idle_shutdown_decision(&self) -> Result<IdleShutdownDecision, RuntimeError> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .idle_shutdown_decision()
    }

    #[cfg(test)]
    pub(crate) fn open_server_request(
        &self,
        draft: ServerRequestDraft,
        now: AppServerTime,
    ) -> OpenRequestOutcome {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .open_server_request(draft, now)
    }

    #[cfg(test)]
    pub(crate) fn pending_server_requests_for_task(
        &self,
        task_id: &TaskId,
    ) -> Vec<openaide_app_server_protocol::snapshot::PendingRequestSnapshot> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .server_requests
            .pending_for_task(task_id)
    }
}
