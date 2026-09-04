use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use crate::protocol::host::HostRequest;
use crate::tasks::product_api::ResolvedSentFile;
use openaide_app_server_protocol::attachment::PreSendAttachment;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::methods::{AGENT_AUTHENTICATE, AGENT_LOGOUT};
use openaide_app_server_protocol::server_requests::SHELL_OPEN_EXTERNAL;
use serde_json::{json, Value};

use crate::app_lifecycle::{LifecycleState, ShutdownCompletion};
use crate::client_lifecycle::{AppServerTime, ClientExpiryOutcome, ConnectionId};
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestDelivery;
#[cfg(test)]
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use crate::shell_file_handles::ShellFileRevealTarget;
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
    pub(crate) fn consume_file_handle(
        &self,
        client_instance_id: &ClientInstanceId,
        file_handle_id: &str,
    ) -> Option<ShellFileRevealTarget> {
        let gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
        gateway.client_hub.client_by_instance(client_instance_id)?;
        gateway
            .shell_file_reveals
            .consume_for_client(client_instance_id, file_handle_id)
    }
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

    /// Delegates periodic storage work without retaining the protocol lock.
    pub fn request_task_storage_maintenance(&self) {
        let workflow = self
            .gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .task_storage_maintenance
            .clone();
        workflow.request_task_storage_maintenance();
    }

    pub fn has_task_navigation_subscribers(&self) -> bool {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .state_stream
            .subscription_count_for_kind(|scope| {
                matches!(
                    scope,
                    openaide_app_server_protocol::state::SubscriptionScope::TaskNavigation {
                        section: openaide_app_server_protocol::task::TaskNavigationSection::Tasks,
                        ..
                    }
                )
            })
            > 0
    }

    pub fn handle_inbound(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let started_at = Instant::now();
        self.handle_inbound_with_completion_clock(connection_id, message, now, || {
            let elapsed_ms = started_at
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX);
            AppServerTime(now.0.saturating_add(elapsed_ms))
        })
    }

    fn handle_inbound_with_completion_clock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> GatewayOutcome {
        if matches!(
            &message,
            InboundProtocolMessage::ClientRequest { method, .. }
                if method == AGENT_AUTHENTICATE || method == AGENT_LOGOUT
        ) {
            return if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. } if method == AGENT_AUTHENTICATE)
            {
                self.handle_agent_authenticate_without_protocol_lock(
                    connection_id,
                    message,
                    now,
                    completion_clock,
                )
            } else {
                self.handle_agent_logout_without_protocol_lock(
                    connection_id,
                    message,
                    now,
                    completion_clock,
                )
            };
        }
        let mut gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
        let outcome = gateway.handle_inbound(connection_id.clone(), message, now);
        // An authenticated request that completes after a long Agent operation is fresh
        // client-liveness evidence. Renew before releasing the protocol lock so the expiry
        // worker cannot invalidate the client between completion and the next heartbeat.
        // Attachment leases deliberately remain tied to request-start transport activity;
        // completion must not revive handles already abandoned during the long request.
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        outcome
    }

    fn handle_agent_authenticate_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> GatewayOutcome {
        let InboundProtocolMessage::ClientRequest {
            id, params, meta, ..
        } = message
        else {
            unreachable!("authenticate path only handles client requests");
        };
        let prepared = {
            let mut gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
            gateway.prepare_agent_authenticate(
                connection_id.clone(),
                id.clone(),
                params,
                meta.clone(),
                now,
            )
        };
        let outcome = match prepared {
            Err(outcome) => *outcome,
            Ok((params, workflow)) => {
                let secret_resolver = if params.secret_env.is_empty()
                    || !connection_id.as_str().starts_with("local-http:")
                {
                    None
                } else {
                    self.client_secret_resolver(&connection_id)
                };
                let result = workflow.authenticate_with_secret_resolver(params, secret_resolver);
                let mut gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
                gateway.finish_agent_authenticate(connection_id.clone(), id, meta, now, result)
            }
        };
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        outcome
    }

    fn client_secret_resolver(
        &self,
        connection_id: &ConnectionId,
    ) -> Option<Arc<dyn crate::agent::AgentSecretResolver>> {
        let gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
        let client_instance_id = gateway
            .client_hub
            .context_for_connection(connection_id)?
            .client_instance_id;
        let delivery = gateway.client_hub.delivery_for(&client_instance_id)?;
        Some(Arc::new(crate::agent::ClientSecretResolver::new(
            gateway.server_requests.clone(),
            client_instance_id,
            delivery,
        )))
    }

    fn handle_agent_logout_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> GatewayOutcome {
        let InboundProtocolMessage::ClientRequest {
            id, params, meta, ..
        } = message
        else {
            unreachable!("logout path only handles client requests");
        };
        let prepared = self
            .gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .prepare_agent_logout(connection_id.clone(), id.clone(), params, meta.clone(), now);
        let outcome = match prepared {
            Err(outcome) => *outcome,
            Ok((params, workflow)) => {
                let result = workflow.logout(params);
                self.gateway
                    .lock()
                    .expect("protocol gateway lock poisoned")
                    .finish_agent_logout(connection_id.clone(), id, meta, now, result)
            }
        };
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        outcome
    }

    #[cfg(test)]
    pub(crate) fn handle_inbound_completed_at(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completed_at: AppServerTime,
    ) -> GatewayOutcome {
        self.handle_inbound_with_completion_clock(connection_id, message, now, || completed_at)
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

    pub fn publish_agent_status_update(&self, now: AppServerTime) -> Vec<GatewayEventDelivery> {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .publish_background_agent_status_update(now)
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

    /// Records an Agent's sign-in URL as Sign-in Flow state that every client renders. The URL is
    /// never opened on the App Server host and never pushed to one arbitrary client.
    pub fn respond_to_acp_host_request(&self, request: &HostRequest) -> Value {
        if request.method != SHELL_OPEN_EXTERNAL {
            return json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": { "code": -32601, "message": "host request is not routed on Local HTTP" },
            });
        }
        let param = |key: &str| {
            request
                .params
                .as_ref()
                .and_then(|params| params.get(key))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(agent_id), Some(url)) = (param("agentId"), param("url")) else {
            return json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": { "code": -32602, "message": "agentId and url are required" },
            });
        };
        let workflow = self
            .gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .agent_authenticate
            .clone();
        // Outside the protocol lock: the workflow notifies the status publisher, which takes it.
        let opened = workflow.record_sign_in_url(&agent_id, url, param("message"));
        json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": { "opened": opened },
        })
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

    pub fn has_initialized_clients(&self) -> bool {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .client_hub
            .has_initialized_clients()
    }

    pub fn has_ever_initialized_clients(&self) -> bool {
        self.gateway
            .lock()
            .expect("protocol gateway lock poisoned")
            .client_hub
            .has_ever_initialized_clients()
    }

    /// Reports the lifecycle-owned terminal condition without exposing client records.
    pub(crate) fn should_shutdown_after_last_client(&self) -> bool {
        let gateway = self.gateway.lock().expect("protocol gateway lock poisoned");
        gateway.lifecycle.state() == LifecycleState::Draining
            && gateway.client_hub.has_ever_initialized_clients()
            && !gateway.client_hub.has_initialized_clients()
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
