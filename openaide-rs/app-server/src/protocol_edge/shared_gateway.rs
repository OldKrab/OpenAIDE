use std::sync::{Arc, Mutex};

use crate::tasks::product_api::ResolvedSentFile;
use openaide_app_server_protocol::attachment::PreSendAttachment;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};

use crate::app_lifecycle::ShutdownCompletion;
use crate::client_lifecycle::{AppServerTime, ClientExpiryOutcome, ConnectionId};
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestDelivery;
#[cfg(test)]
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use crate::task_events::TaskUpdate;
use openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot;

use super::{
    AppServerProbeFacts, GatewayEventDelivery, GatewayOutcome, InboundProtocolMessage, RpcGateway,
};

#[derive(Clone)]
pub struct SharedRpcGateway {
    gateway: Arc<Mutex<RpcGateway>>,
}

impl SharedRpcGateway {
    pub fn new(gateway: RpcGateway) -> Self {
        Self {
            gateway: Arc::new(Mutex::new(gateway)),
        }
    }

    /// Delegates scheduling so timer and Send requests share one coalescing owner.
    pub fn request_native_session_catalog_refresh(&self) {
        let workflow = self
            .gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .agent_list_sessions
            .clone();
        workflow.request_native_session_catalog_refresh();
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

    pub(crate) fn client_is_initialized(&self, client_instance_id: &ClientInstanceId) -> bool {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .client_hub
            .client_by_instance(client_instance_id)
            .is_some()
    }

    /// Converts a completed Web upload into an opaque draft attachment handle.
    pub(crate) fn create_uploaded_file_reference(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: TaskId,
        path: String,
        label: String,
    ) -> Result<PreSendAttachment, ProtocolError> {
        let gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
        if gateway
            .client_hub
            .client_by_instance(client_instance_id)
            .is_none()
        {
            return Err(ProtocolError {
                code: openaide_app_server_protocol::errors::ProtocolErrorCode::NotInitialized,
                message: "client connection is not initialized".to_string(),
                recoverable: true,
                target: None,
            });
        }
        gateway.attachments.create_uploaded_file_reference(
            client_instance_id,
            &task_id,
            path,
            label,
        )
    }

    /// Resolves only a file already persisted in a Task message visible to this client.
    pub(crate) fn resolve_sent_file(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        message_id: &str,
        attachment_index: usize,
    ) -> Result<ResolvedSentFile, ProtocolError> {
        let gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
        if gateway
            .client_hub
            .client_by_instance(client_instance_id)
            .is_none()
        {
            return Err(ProtocolError {
                code: openaide_app_server_protocol::errors::ProtocolErrorCode::NotInitialized,
                message: "client connection is not initialized".to_string(),
                recoverable: true,
                target: None,
            });
        }
        gateway.attachments.resolve_sent_file(
            client_instance_id,
            task_id,
            message_id,
            attachment_index,
        )
    }

    /// Keeps an initialized client live while its authenticated transport is active.
    pub fn observe_connection_activity(
        &self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> bool {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .observe_connection_activity(connection_id, now)
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

    pub fn publish_worktree_repository_update(
        &self,
        repository: WorktreeRepositorySnapshot,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .publish_background_worktree_repository_update(repository, now)
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
