use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use crate::protocol::host::HostRequest;
use crate::tasks::product_api::ResolvedSentFile;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::methods::{
    AGENT_AUTHENTICATE, AGENT_LOGOUT, FILE_VIEWER_CHANGES, FILE_VIEWER_DIFF,
    FILE_VIEWER_EXISTING_FILES, FILE_VIEWER_LIST_DIRECTORY, FILE_VIEWER_OPEN,
    FILE_VIEWER_OPEN_FROM_HANDLE, FILE_VIEWER_REFRESH, FILE_VIEWER_SEARCH, TASK_IMAGE,
    TASK_NAVIGATION_SEARCH,
};
use openaide_app_server_protocol::server_requests::SHELL_OPEN_EXTERNAL;
use serde_json::{json, Value};

use crate::app_lifecycle::{LifecycleState, ShutdownCause, ShutdownCompletion};
use crate::client_lifecycle::{AppServerTime, ClientExpiryOutcome, ConnectionId};
use crate::protocol::errors::RuntimeError;
#[cfg(test)]
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use crate::shell_file_handles::ShellFileRevealTarget;
use crate::task_events::TaskUpdate;
#[cfg(test)]
use crate::tasks::product_api::NativeCatalogRefreshRequester;
use openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot;

use super::outbox::{OutboundMessage, Outbox, ProtocolExchange, ReplyRoute};
use super::stdio::wire::InboundRejection;
use super::task_update_publisher::TaskUpdatePublication;
use super::{
    AppServerProbeFacts, DeliverySignal, GatewayOutcome, InboundProtocolMessage, RpcGateway,
};

/// A client request waiting this long for the protocol lock is user-visible latency:
/// every request and publication on this gateway is serialized behind one mutex.
const SLOW_LOCK_WAIT: Duration = Duration::from_millis(100);

mod lock_free;
mod navigation_search;
mod relay;
mod task_image;
mod uploads;

#[derive(Clone)]
pub struct SharedRpcGateway {
    gateway: Arc<Mutex<RpcGateway>>,
    /// The gateway's outbox, reachable without the protocol lock so transports can drain it.
    outbox: Outbox,
    deliveries: DeliverySignal,
    slow_lock_wait: Duration,
}

/// A handled inbound message: the outcome, and the protocol lock it was produced under. The
/// outcome joins the outbox before this guard is released.
type Handled<'a> = (SignallingGateway<'a>, GatewayOutcome);

/// Gateway access that may have queued client deliveries. Releasing it wakes
/// transport links, so no publication site has to remember to signal.
struct SignallingGateway<'a> {
    gateway: Option<MutexGuard<'a, RpcGateway>>,
    deliveries: &'a DeliverySignal,
}

impl Deref for SignallingGateway<'_> {
    type Target = RpcGateway;

    fn deref(&self) -> &RpcGateway {
        self.gateway.as_ref().expect("gateway guard is held")
    }
}

impl DerefMut for SignallingGateway<'_> {
    fn deref_mut(&mut self) -> &mut RpcGateway {
        self.gateway.as_mut().expect("gateway guard is held")
    }
}

impl Drop for SignallingGateway<'_> {
    fn drop(&mut self) {
        // Whatever this holder changed reaches the views before the lock is released, so no
        // later publication can overtake the frames it owes.
        if let Some(gateway) = self.gateway.as_mut() {
            if gateway.navigation.is_dirty() {
                gateway.flush_navigation(crate::client_lifecycle::AppServerTime::now());
            }
        }
        // Unlock first: a woken link drains through the same mutex.
        self.gateway.take();
        self.deliveries.notify();
    }
}

impl SharedRpcGateway {
    /// Resolve only a live client's own viewer capability, without holding the gateway during I/O.
    pub(crate) fn file_viewer_download_path(
        &self,
        client_instance_id: &ClientInstanceId,
        handle: &str,
    ) -> Option<std::path::PathBuf> {
        let gateway = self.lock();
        gateway.client_hub.client_by_instance(client_instance_id)?;
        gateway
            .file_viewer
            .download_path(client_instance_id, &handle.into())
    }

    pub(crate) fn consume_file_handle(
        &self,
        client_instance_id: &ClientInstanceId,
        file_handle_id: &str,
    ) -> Option<ShellFileRevealTarget> {
        let gateway = self.lock();
        gateway.client_hub.client_by_instance(client_instance_id)?;
        gateway
            .shell_file_reveals
            .consume_for_client(client_instance_id, file_handle_id)
    }
    pub fn new(gateway: RpcGateway) -> Self {
        Self {
            outbox: gateway.outbox.clone(),
            gateway: Arc::new(Mutex::new(gateway)),
            deliveries: DeliverySignal::default(),
            slow_lock_wait: SLOW_LOCK_WAIT,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_slow_lock_wait(mut self, threshold: Duration) -> Self {
        self.slow_lock_wait = threshold;
        self
    }

    /// Lets a test observe from inside a read whether the protocol lock is held.
    #[cfg(test)]
    pub(crate) fn try_lock_for_test(&self) -> Option<MutexGuard<'_, RpcGateway>> {
        self.gateway.try_lock().ok()
    }

    /// Wake source for transport links waiting on this gateway's deliveries.
    pub fn delivery_signal(&self) -> DeliverySignal {
        self.deliveries.clone()
    }

    fn lock(&self) -> SignallingGateway<'_> {
        SignallingGateway {
            gateway: Some(self.lock_quiet()),
            deliveries: &self.deliveries,
        }
    }

    /// Access used by the links themselves. Draining and liveness renewal must
    /// stay silent, otherwise each woken link would wake every other one.
    fn lock_quiet(&self) -> MutexGuard<'_, RpcGateway> {
        self.gateway.lock().expect("protocol gateway lock poisoned")
    }

    /// Delegates scheduling so timer and Send requests share one coalescing owner.
    #[cfg(test)]
    pub(crate) fn request_native_session_catalog_refresh(
        &self,
        requester: NativeCatalogRefreshRequester,
    ) {
        let workflow = self.lock().agent_list_sessions.clone();
        workflow.request_native_session_catalog_refresh(requester);
    }

    /// The coordinator owns the clock, so every kind of run resets the periodic refresh.
    /// Asked every second, and nothing here can be pending for a link, so it stays silent.
    pub fn request_due_native_session_catalog_refresh(&self, interval: Duration) {
        let workflow = self.lock_quiet().agent_list_sessions.clone();
        workflow.request_due_native_session_catalog_refresh(interval);
    }

    /// Delegates periodic storage work without retaining the protocol lock.
    pub fn request_task_storage_maintenance(&self) {
        let workflow = self.lock().task_storage_maintenance.clone();
        workflow.request_task_storage_maintenance();
    }

    /// Asked every second, and nothing here can be pending for a link, so it stays silent.
    pub fn request_scheduled_queue_delivery(&self) {
        let workflow = self.lock_quiet().task_send.clone();
        workflow.request_scheduled_queue_delivery();
    }

    /// Asked every second and only reads, so it stays silent.
    pub fn has_task_navigation_subscribers(&self) -> bool {
        self.lock_quiet().navigation.has_tasks_view()
    }

    /// Runs one inbound message and appends everything it produced to the outbound sequences
    /// before the protocol lock is released, so no later publication can overtake it. Returns
    /// whether a reply was queued.
    pub fn dispatch_inbound(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
    ) -> bool {
        let clock = completion_clock(now);
        self.handle_queued(connection_id, message, now, clock, |_| ())
            .1
    }

    /// Like [`Self::dispatch_inbound`], and also returns a copy of the outcome to inspect.
    #[cfg(test)]
    pub(crate) fn handle_inbound(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let clock = completion_clock(now);
        self.handle_queued(connection_id, message, now, clock, Clone::clone)
            .0
    }

    fn handle_queued<R>(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
        inspect: impl FnOnce(&GatewayOutcome) -> R,
    ) -> (R, bool) {
        let route = ReplyRoute::for_message(&message);
        let (mut gateway, outcome) =
            self.handle_with_completion_clock(connection_id, message, now, completion_clock);
        // Frames for what this request changed go out ahead of its reply, but after the frames its
        // handler already stamped (those are in the outcome): the client drops a frame whose
        // revision is not above the held one, and a later frame carries no bodies the earlier
        // one introduced, so the two must reach the outbox in revision order.
        let outcome = gateway.append_navigation_flush(outcome, now);
        let inspected = inspect(&outcome);
        let replied = gateway.outbox.push_outcome(outcome, route);
        drop(gateway);
        (inspected, replied)
    }

    fn handle_with_completion_clock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. }
            if matches!(method.as_str(), FILE_VIEWER_LIST_DIRECTORY | FILE_VIEWER_SEARCH | FILE_VIEWER_EXISTING_FILES | FILE_VIEWER_CHANGES | FILE_VIEWER_DIFF))
        {
            return self.handle_project_files_without_protocol_lock(
                connection_id,
                message,
                now,
                completion_clock,
            );
        }
        if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. }
            if matches!(method.as_str(), FILE_VIEWER_OPEN | FILE_VIEWER_OPEN_FROM_HANDLE | FILE_VIEWER_REFRESH))
        {
            return self.handle_file_viewer_without_protocol_lock(
                connection_id,
                message,
                now,
                completion_clock,
            );
        }
        if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. } if super::device_handlers::is_relay_method(method))
        {
            return self.handle_relay_without_protocol_lock(
                connection_id,
                message,
                now,
                completion_clock,
            );
        }
        if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. } if method == TASK_IMAGE)
        {
            return self.handle_task_image_without_protocol_lock(
                connection_id,
                message,
                now,
                completion_clock,
            );
        }
        if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. } if method == TASK_NAVIGATION_SEARCH)
        {
            return self.handle_navigation_search_without_protocol_lock(
                connection_id,
                message,
                now,
                completion_clock,
            );
        }
        if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. } if method == AGENT_AUTHENTICATE)
        {
            return self.handle_agent_authenticate_without_protocol_lock(
                connection_id,
                message,
                now,
                completion_clock,
            );
        }
        if matches!(&message, InboundProtocolMessage::ClientRequest { method, .. } if method == AGENT_LOGOUT)
        {
            return self.handle_agent_logout_without_protocol_lock(
                connection_id,
                message,
                now,
                completion_clock,
            );
        }
        let wait_started_at = Instant::now();
        let mut gateway = self.lock();
        self.log_slow_client_lock_wait(&connection_id, &message, wait_started_at.elapsed());
        let outcome = gateway.handle_inbound(connection_id.clone(), message, now);
        // An authenticated request that completes after a long Agent operation is fresh
        // client-liveness evidence. Renew before releasing the protocol lock so the expiry
        // worker cannot invalidate the client between completion and the next heartbeat.
        // Attachment leases deliberately remain tied to request-start transport activity;
        // completion must not revive handles already abandoned during the long request.
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        (gateway, outcome)
    }

    #[cfg(test)]
    pub(crate) fn handle_inbound_completed_at(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completed_at: AppServerTime,
    ) -> GatewayOutcome {
        self.handle_queued(connection_id, message, now, || completed_at, Clone::clone)
            .0
    }

    pub fn probe_facts(&self) -> AppServerProbeFacts {
        self.lock().probe_facts()
    }

    pub fn connection_is_initialized(&self, connection_id: &ConnectionId) -> bool {
        self.lock_quiet()
            .client_hub
            .context_for_connection(connection_id)
            .is_some()
    }

    pub(crate) fn client_is_initialized(&self, client_instance_id: &ClientInstanceId) -> bool {
        self.lock()
            .client_hub
            .client_by_instance(client_instance_id)
            .is_some()
    }

    /// Resolves only a file already persisted in a Task message visible to this client.
    pub(crate) fn resolve_sent_file(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        message_id: &str,
        attachment_index: usize,
    ) -> Result<ResolvedSentFile, ProtocolError> {
        let gateway = self.lock();
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

    /// Keeps an initialized client live while its link is active, and queues the server
    /// requests it can now answer. Returns false when no initialized client owns the connection.
    pub fn observe_link_activity(&self, connection_id: &ConnectionId, now: AppServerTime) -> bool {
        let wait_started_at = Instant::now();
        let mut gateway = self.lock_quiet();
        // Every link takes this before writing a frame, so a slow wait here is latency
        // on the server-to-client path that no request duration shows.
        self.log_slow_lock_wait(connection_id, "delivery", None, wait_started_at.elapsed());
        if !gateway.observe_connection_activity(connection_id, now) {
            return false;
        }
        let requests = gateway.drain_server_requests_for_connection(connection_id, now);
        gateway.outbox.push_server_requests(requests);
        true
    }

    /// Queues the server requests this connection can now answer, without counting as client
    /// activity.
    pub fn queue_deliverable_server_requests(
        &self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) {
        let mut gateway = self.lock_quiet();
        let requests = gateway.drain_server_requests_for_connection(connection_id, now);
        gateway.outbox.push_server_requests(requests);
    }

    /// Answers a message the transport refused before dispatch, in sequence with everything
    /// already queued for the connection.
    pub(crate) fn queue_rejection(
        &self,
        connection_id: &ConnectionId,
        rejection: InboundRejection,
    ) {
        self.outbox.push(connection_id, rejection.into_outbound());
    }

    /// Moves the connection's outbound sequence to `write`. See [`Outbox::transfer`].
    pub(crate) fn transfer_outbound<R>(
        &self,
        connection_id: &ConnectionId,
        write: impl FnOnce(Vec<OutboundMessage>) -> R,
    ) -> R {
        self.outbox.transfer(connection_id, write)
    }

    /// The Task attention cursor this connection would be answered with now.
    ///
    /// A held `taskAttention/wait` calls this on every delivery wake, so it renews
    /// the client's liveness like any transport activity, never signals other links,
    /// and reads Task storage outside the protocol lock. `None` ends the hold: the
    /// client is no longer initialized, or the read failed and the dispatched
    /// request reports why.
    pub(crate) fn task_attention_cursor(
        &self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> Option<String> {
        let snapshots = {
            let mut gateway = self.lock_quiet();
            if !gateway.observe_connection_activity(connection_id, now) {
                return None;
            }
            gateway.task_snapshots.clone()
        };
        super::task_attention_handlers::current_task_attention(snapshots.as_ref())
            .ok()
            .map(|result| result.cursor)
    }

    /// Publishes one committed Task update. Deliveries reach links through the delivery
    /// signal; the caller receives only lock timing for its batch diagnostics.
    pub fn publish_committed_task_update(
        &self,
        update: &TaskUpdate,
        now: AppServerTime,
    ) -> TaskUpdatePublication {
        let wait_started_at = Instant::now();
        let mut gateway = self.lock();
        let held_since = Instant::now();
        gateway.publish_task_update(update, now);
        TaskUpdatePublication {
            lock_wait: held_since.duration_since(wait_started_at),
            lock_held: held_since.elapsed(),
        }
    }

    fn log_slow_client_lock_wait(
        &self,
        connection_id: &ConnectionId,
        message: &InboundProtocolMessage,
        wait: Duration,
    ) {
        let (kind, method) = match message {
            InboundProtocolMessage::ClientRequest { method, .. } => ("request", Some(method)),
            InboundProtocolMessage::ClientNotification { method, .. } => {
                ("notification", Some(method))
            }
            InboundProtocolMessage::ClientResponse { .. } => ("response", None),
        };
        self.log_slow_lock_wait(connection_id, kind, method, wait);
    }

    fn log_slow_lock_wait(
        &self,
        connection_id: &ConnectionId,
        kind: &'static str,
        method: Option<&String>,
        wait: Duration,
    ) {
        if wait < self.slow_lock_wait {
            return;
        }
        crate::logging::info(
            "gateway_lock_wait_slow",
            json!({
                "operation": "protocol_lock",
                "connection_id": connection_id.as_str(),
                "transport_operation_kind": kind,
                "method": method,
                "wait_ms": wait.as_millis(),
            }),
        );
    }

    pub fn publish_worktree_repository_update(&self, repository: WorktreeRepositorySnapshot) {
        self.lock()
            .publish_background_worktree_repository_update(repository);
    }

    pub fn publish_agent_status_update(&self) {
        self.lock().publish_background_agent_status_update();
    }

    pub fn remote_devices(&self) -> Option<crate::remote_devices::RemoteDevices> {
        self.lock_quiet().remote_device_service()
    }

    pub fn publish_device_collection_update(&self) {
        self.lock().publish_background_device_collection_update();
    }

    pub fn terminals(&self) -> crate::terminals::Terminals {
        self.lock_quiet().terminals.clone()
    }

    pub fn publish_terminal_collection_update(&self) {
        self.lock().publish_background_terminal_collection_update();
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
        let workflow = self.lock().agent_authenticate.clone();
        // Outside the protocol lock: the workflow notifies the status publisher, which takes it.
        let opened = workflow.record_sign_in_url(&agent_id, url, param("message"));
        json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": { "opened": opened },
        })
    }

    #[cfg(test)]
    pub(crate) fn drain_event_deliveries_for_connection(
        &self,
        connection_id: &ConnectionId,
    ) -> Vec<openaide_app_server_protocol::events::AppServerEvent> {
        self.lock_quiet()
            .drain_event_deliveries_for_connection(connection_id)
    }

    /// Queues the connection's deliverable server requests and takes them, dropping anything
    /// else queued for it.
    #[cfg(test)]
    pub(crate) fn drain_server_requests_for_connection(
        &self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> Vec<openaide_app_server_protocol::envelopes::ServerRequestEnvelope<serde_json::Value>>
    {
        self.queue_deliverable_server_requests(connection_id, now);
        self.transfer_outbound(connection_id, |messages| {
            messages
                .into_iter()
                .filter_map(|message| match message {
                    OutboundMessage::ServerRequest(envelope) => Some(envelope),
                    _ => None,
                })
                .collect()
        })
    }

    pub fn expire_client_after_reconnect_grace(
        &self,
        client_instance_id: &ClientInstanceId,
        now: AppServerTime,
    ) -> ClientExpiryOutcome {
        self.lock()
            .expire_client_after_reconnect_grace(client_instance_id, now)
    }

    pub fn expire_inactive_clients(&self, now: AppServerTime) -> Vec<ClientExpiryOutcome> {
        self.lock().expire_inactive_clients(now)
    }

    pub fn has_initialized_clients(&self) -> bool {
        self.lock().client_hub.has_initialized_clients()
    }

    pub fn has_ever_initialized_clients(&self) -> bool {
        self.lock().client_hub.has_ever_initialized_clients()
    }

    /// Reports the lifecycle-owned terminal condition without exposing client records.
    pub(crate) fn should_shutdown_after_last_client(&self) -> bool {
        let gateway = self.lock();
        gateway.lifecycle.state() == LifecycleState::Draining
            && gateway.client_hub.has_ever_initialized_clients()
            && !gateway.client_hub.has_initialized_clients()
    }

    pub fn shutdown(&self, cause: ShutdownCause) -> Result<ShutdownCompletion, RuntimeError> {
        // Terminals never keep App Server alive (ADR-0071); clients warn before stopping it.
        // Close them outside the protocol lock.
        self.terminals().close_all();
        self.lock().shutdown(cause)
    }

    #[cfg(test)]
    pub(crate) fn open_server_request(
        &self,
        draft: ServerRequestDraft,
        now: AppServerTime,
    ) -> OpenRequestOutcome {
        self.lock().open_server_request(draft, now)
    }

    #[cfg(test)]
    pub(crate) fn pending_server_requests_for_task(
        &self,
        task_id: &TaskId,
    ) -> Vec<openaide_app_server_protocol::snapshot::PendingRequestSnapshot> {
        self.lock().server_requests.pending_for_task(task_id)
    }
}

/// The request's start time advanced by its wall-clock duration, read when it completes.
fn completion_clock(now: AppServerTime) -> impl FnOnce() -> AppServerTime {
    let started_at = Instant::now();
    move || {
        let elapsed_ms = started_at
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        AppServerTime(now.0.saturating_add(elapsed_ms))
    }
}

impl ProtocolExchange for SharedRpcGateway {
    fn dispatch(&self, connection_id: ConnectionId, message: InboundProtocolMessage) -> bool {
        self.dispatch_inbound(connection_id, message, AppServerTime::now())
    }

    fn observe_link(&self, connection_id: &ConnectionId) -> bool {
        self.observe_link_activity(connection_id, AppServerTime::now())
    }

    fn transfer<R>(
        &self,
        connection_id: &ConnectionId,
        write: impl FnOnce(Vec<OutboundMessage>) -> R,
    ) -> R {
        self.transfer_outbound(connection_id, write)
    }
}
