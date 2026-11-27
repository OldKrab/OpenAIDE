use openaide_app_server_protocol::client::ShellKind;
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::envelopes::{ErrorEnvelope, ResponseMeta};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::events::AppServerEventPayload;
use openaide_app_server_protocol::ids::{RequestId, TaskId};
use openaide_app_server_protocol::server_requests::{
    PendingRequestResolution, PendingRequestResolveParams, PendingRequestResolveResult,
};
use openaide_app_server_protocol::snapshot::{
    PendingRequestScope, PendingRequestSnapshot, TaskSnapshot,
};

use crate::app_lifecycle::{
    LifecycleState, ShutdownCause, ShutdownCompletion, ShutdownRequestOutcome,
};
#[cfg(test)]
use crate::client_lifecycle::Delivery;
use crate::client_lifecycle::{AppServerTime, ClientContext, ClientExpiryOutcome, ConnectionId};
use crate::global_state::GlobalAreas;
use crate::protocol::errors::RuntimeError;
use crate::protocol_edge::{
    responses, GatewayEventDelivery, GatewayOutcome, GatewayResponse, RpcGateway,
};
use crate::server_requests::ServerRequestDelivery;
#[cfg(test)]
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use crate::server_requests::{ResponseOutcome, ServerRequestAnswer};

impl RpcGateway {
    pub(super) fn handle_pending_request_resolve(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: serde_json::Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<PendingRequestResolveParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(
                    openaide_app_server_protocol::methods::PENDING_REQUEST_RESOLVE.to_string(),
                ),
            );
        };
        let answer = match params.resolution {
            PendingRequestResolution::Permission { option_id } => {
                ServerRequestAnswer::Result(serde_json::json!({ "optionId": option_id }))
            }
            PendingRequestResolution::Question { response } => ServerRequestAnswer::Result(
                serde_json::to_value(response).expect("question resolution serializes"),
            ),
        };
        let outcome = self.answer_request(&context, params.request_id, answer, now);
        let ResponseOutcome::Accepted { scope, .. } = outcome else {
            return self.error(
                connection_id,
                id,
                meta,
                server_request_response_error(outcome),
            );
        };
        let events = self.publish_request_resolution(scope, now);
        responses::result_with_events(
            connection_id,
            id,
            meta,
            PendingRequestResolveResult::default(),
            events,
        )
    }

    pub fn shutdown(&mut self, cause: ShutdownCause) -> Result<ShutdownCompletion, RuntimeError> {
        let previous = self.lifecycle.state();
        if matches!(
            self.lifecycle.request_shutdown(),
            ShutdownRequestOutcome::AlreadyStopping
        ) {
            return Ok(ShutdownCompletion::CleanRelease);
        }
        crate::logging::info(
            "app_server_lifecycle_transition",
            serde_json::json!({
                "from_status": format!("{previous:?}"),
                "to_status": "Stopping",
                "reason_code": cause.reason_code(),
            }),
        );
        match self.shutdown.shutdown() {
            Ok(()) => Ok(self.lifecycle.complete_shutdown(true)),
            Err(error) => {
                let _ = self.lifecycle.complete_shutdown(false);
                Err(error)
            }
        }
    }

    pub fn handle_transport_closed(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> crate::client_lifecycle::TransportClosedOutcome {
        let outcome = self.client_hub.observe_transport_closed(connection_id, now);
        crate::logging::info(
            "client_transport_closed",
            serde_json::json!({
                "connection_id": connection_id.as_str(),
                "outcome": format!("{:?}", outcome),
            }),
        );
        if let crate::client_lifecycle::TransportClosedOutcome::EnteredReconnectGrace {
            client_instance_id,
            ..
        } = &outcome
        {
            self.server_requests
                .observe_transport_unavailable(client_instance_id, now);
        }
        outcome
    }

    pub(super) fn detach_client(
        &mut self,
        client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        now: AppServerTime,
    ) {
        crate::logging::info(
            "client_detach_started",
            serde_json::json!({ "client_instance_id": client_instance_id }),
        );
        let connection_id = self
            .client_hub
            .client_by_instance(client_instance_id)
            .map(|client| client.connection_id);
        let crate::client_lifecycle::DetachOutcome::Detached { last_client, .. } =
            self.client_hub.detach(client_instance_id)
        else {
            return;
        };
        if let Some(connection_id) = connection_id {
            self.outbox.discard(&connection_id, "client_detached");
        }
        self.server_requests
            .observe_client_expired(client_instance_id, now);
        self.state_stream
            .drop_client_subscriptions(client_instance_id);
        self.navigation.drop_client(client_instance_id);
        if let Err(error) = self.task_release.release_expired_client(client_instance_id) {
            crate::logging::error(
                "explicit_client_detach_task_release_failed",
                serde_json::json!({ "error": error.message }),
            );
        }
        self.remove_expired_client_workspace_roots(client_instance_id);
        self.file_viewer.release_client(client_instance_id);
        crate::logging::info(
            "client_detach_completed",
            serde_json::json!({ "client_instance_id": client_instance_id }),
        );
        if last_client {
            self.begin_draining_after_last_client("explicit_detach");
        }
    }

    pub fn expire_client_after_reconnect_grace(
        &mut self,
        client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        now: AppServerTime,
    ) -> ClientExpiryOutcome {
        let connection_id = self
            .client_hub
            .client_by_instance(client_instance_id)
            .map(|client| client.connection_id);
        let outcome = self.client_hub.expire_after_grace(client_instance_id, now);
        crate::logging::info(
            "client_reconnect_grace_evaluated",
            serde_json::json!({
                "client_instance_id": client_instance_id,
                "outcome": format!("{:?}", outcome),
            }),
        );
        if let ClientExpiryOutcome::Expired { .. } = &outcome {
            if let Some(connection_id) = &connection_id {
                self.outbox.discard(connection_id, "client_expired");
            }
            self.server_requests
                .observe_client_expired(client_instance_id, now);
            self.state_stream
                .drop_client_subscriptions(client_instance_id);
            self.navigation.drop_client(client_instance_id);
            if let Err(error) = self.task_release.release_expired_client(client_instance_id) {
                crate::logging::error(
                    "prepared_task_lease_expiry_release_failed",
                    serde_json::json!({ "error": error.message }),
                );
            }
            self.remove_expired_client_workspace_roots(client_instance_id);
        }
        if matches!(
            &outcome,
            ClientExpiryOutcome::Expired {
                last_client: true,
                ..
            }
        ) {
            self.begin_draining_after_last_client("reconnect_grace_expired");
        }
        outcome
    }

    pub fn expire_inactive_clients(&mut self, now: AppServerTime) -> Vec<ClientExpiryOutcome> {
        let batch = self.client_hub.expire_inactive_clients(now);
        // A transport close logs its own grace entry; silence is only seen here.
        for (client_instance_id, expires_at) in &batch.entered_reconnect_grace {
            crate::logging::info(
                "client_reconnect_grace_entered",
                serde_json::json!({
                    "client_instance_id": client_instance_id,
                    "reason": "heartbeat_timeout",
                    "grace_ms": expires_at.0.saturating_sub(now.0),
                }),
            );
        }
        if !batch.expired.is_empty() {
            crate::logging::info(
                "client_liveness_expired",
                serde_json::json!({
                    "client_count": batch.expired.len(),
                    "client_instance_ids": batch.expired,
                    "last_client_expired": batch.last_client_expired,
                }),
            );
        }
        for connection_id in &batch.expired_connections {
            self.outbox.discard(connection_id, "client_expired");
        }
        let mut projects_changed = false;
        for client_instance_id in &batch.expired {
            self.server_requests
                .observe_client_expired(client_instance_id, now);
            self.state_stream
                .drop_client_subscriptions(client_instance_id);
            self.navigation.drop_client(client_instance_id);
            if let Err(error) = self.task_release.release_expired_client(client_instance_id) {
                crate::logging::error(
                    "prepared_task_lease_expiry_release_failed",
                    serde_json::json!({ "error": error.message }),
                );
            }
            projects_changed |= self
                .project_roots
                .remove_client_workspace_roots(client_instance_id);
        }
        if projects_changed {
            self.publish_global_state(GlobalAreas::PROJECTS);
        }
        if batch.last_client_expired {
            self.begin_draining_after_last_client("client_liveness_expired");
        }
        let last_expired_index = batch
            .last_client_expired
            .then(|| batch.expired.len().saturating_sub(1));
        batch
            .expired
            .into_iter()
            .enumerate()
            .map(|(index, client_instance_id)| ClientExpiryOutcome::Expired {
                client_instance_id,
                last_client: Some(index) == last_expired_index,
            })
            .collect()
    }

    /// App lifecycle owns the transition after client lifecycle reports the fact.
    fn begin_draining_after_last_client(&mut self, reason: &'static str) {
        let previous = self.lifecycle.state();
        self.lifecycle.begin_draining();
        if previous != LifecycleState::Draining
            && self.lifecycle.state() == LifecycleState::Draining
        {
            crate::logging::info(
                "app_server_lifecycle_transition",
                serde_json::json!({
                    "from_status": format!("{previous:?}"),
                    "to_status": "Draining",
                    "reason_code": reason,
                }),
            );
        }
    }

    fn remove_expired_client_workspace_roots(
        &mut self,
        client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
    ) {
        if self
            .project_roots
            .remove_client_workspace_roots(client_instance_id)
        {
            self.publish_global_state(GlobalAreas::PROJECTS);
        }
    }

    /// Takes the connection's queued events and drops anything else queued for it.
    #[cfg(test)]
    pub(crate) fn drain_event_deliveries_for_connection(
        &mut self,
        connection_id: &ConnectionId,
    ) -> Vec<openaide_app_server_protocol::events::AppServerEvent> {
        self.outbox.transfer(connection_id, |messages| {
            messages
                .into_iter()
                .filter_map(|message| match message {
                    super::outbox::OutboundMessage::Event(event) => Some(*event),
                    _ => None,
                })
                .collect()
        })
    }

    /// Takes the connection's queued `sync` changes and drops anything else queued for it.
    #[cfg(test)]
    pub(crate) fn drain_sync_for_connection(
        &mut self,
        connection_id: &ConnectionId,
    ) -> Vec<openaide_app_server_protocol::global_state::GlobalStateChanges> {
        self.outbox.transfer(connection_id, |messages| {
            messages
                .into_iter()
                .filter_map(|message| match message {
                    super::outbox::OutboundMessage::Sync(sync) => Some(sync.changes),
                    _ => None,
                })
                .collect()
        })
    }

    #[cfg(test)]
    pub(crate) fn open_server_request(
        &mut self,
        draft: ServerRequestDraft,
        now: AppServerTime,
    ) -> OpenRequestOutcome {
        let deliveries = self.eligible_deliveries_for_request(&draft);
        self.server_requests.open(draft, deliveries, now)
    }

    pub(crate) fn drain_server_requests_for_connection(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> Vec<ServerRequestDelivery> {
        let Some(context) = self.client_hub.context_for_connection(connection_id) else {
            return Vec::new();
        };
        self.server_requests.observe_responder_available(
            self.client_hub
                .delivery_for(&context.client_instance_id)
                .expect("connected client must have a delivery"),
            now,
        )
    }

    pub(super) fn handle_client_response(
        &mut self,
        connection_id: ConnectionId,
        request_id: String,
        answer: ServerRequestAnswer,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return GatewayOutcome::Noop;
        };
        let terminal_exchange = self
            .server_requests
            .is_auth_terminal_exchange(&RequestId::from(request_id.clone()));
        let outcome =
            self.answer_request(&context, RequestId::from(request_id.clone()), answer, now);
        let ResponseOutcome::Accepted { scope, .. } = outcome else {
            return GatewayOutcome::Respond {
                connection_id,
                id: request_id,
                response: GatewayResponse::Error(Box::new(ErrorEnvelope::new(
                    server_request_response_error(outcome),
                    ResponseMeta::default(),
                ))),
                events: Vec::new(),
                server_requests: Vec::new(),
            };
        };

        // Terminal frames are transport exchanges, not product-state changes. Publishing a
        // Client snapshot for every keystroke would turn transient I/O into state traffic.
        if terminal_exchange {
            return GatewayOutcome::Noop;
        }
        let events = self.publish_request_resolution(scope, now);
        if events.is_empty() {
            GatewayOutcome::Noop
        } else {
            GatewayOutcome::Respond {
                connection_id,
                id: String::new(),
                response: GatewayResponse::Result(serde_json::Value::Null),
                events,
                server_requests: Vec::new(),
            }
        }
    }

    pub(super) fn add_pending_to_subscription_snapshot(
        &mut self,
        snapshot: &mut openaide_app_server_protocol::state::SubscriptionSnapshot,
    ) {
        use openaide_app_server_protocol::state::SubscriptionSnapshot;
        let SubscriptionSnapshot::Task { task, .. } = snapshot else {
            return;
        };
        *task = self.task_with_pending_requests(task.clone());
        let (task_id, requests) = (task.task.task_id.clone(), &task.pending_requests);
        if !requests.is_empty() {
            // One subscriber's empty baseline says nothing about what other clients hold,
            // so a snapshot can only add visibility; the Task stream withdraws it.
            let request_count = requests.len();
            self.mark_task_requests_visible(&task_id, request_count, "subscription_snapshot");
        }
    }

    /// Records the request set just published on a Task's ordered stream. Every subscriber
    /// receives that set, so an empty one ends the Task's visible requests.
    pub(super) fn record_published_task_requests(
        &mut self,
        task_id: &TaskId,
        requests: &[PendingRequestSnapshot],
        source: &'static str,
    ) {
        if !requests.is_empty() {
            self.mark_task_requests_visible(task_id, requests.len(), source);
        } else if self.tasks_with_visible_requests.remove(task_id) {
            log_task_requests_visibility(task_id, false, 0, source);
        }
    }

    fn mark_task_requests_visible(
        &mut self,
        task_id: &TaskId,
        request_count: usize,
        source: &'static str,
    ) {
        if self.tasks_with_visible_requests.insert(task_id.clone()) {
            log_task_requests_visibility(task_id, true, request_count, source);
        }
    }

    pub(super) fn task_with_pending_requests(&self, mut task: TaskSnapshot) -> TaskSnapshot {
        task.pending_requests = self.server_requests.pending_for_task(&task.task.task_id);
        task
    }

    /// Answers a pending request for `context`'s client after the authorization rule
    /// (ADR-0086): any authenticated client may answer a Task's Permission or Question, except
    /// that a VS Code client answers only for Tasks of the Projects it works in. Nothing
    /// here depends on which Tasks the client has open.
    fn answer_request(
        &mut self,
        context: &ClientContext,
        request_id: RequestId,
        answer: ServerRequestAnswer,
        now: AppServerTime,
    ) -> ResponseOutcome {
        if let Some(denied) = self.deny_out_of_project_answer(context, &request_id) {
            return denied;
        }
        self.server_requests.handle_response(
            context.client_instance_id.clone(),
            request_id,
            answer,
            now,
        )
    }

    fn deny_out_of_project_answer(
        &self,
        context: &ClientContext,
        request_id: &RequestId,
    ) -> Option<ResponseOutcome> {
        if context.shell.kind != ShellKind::VscodeExtension {
            return None;
        }
        let task_id = self.server_requests.task_of_pending_request(request_id)?;
        let task_project = self.task_snapshots.project_of(&task_id);
        let allowed = task_project.as_ref().is_some_and(|project| {
            self.project_roots
                .client_project_ids(&context.client_instance_id)
                .contains(project)
        });
        if allowed {
            return None;
        }
        crate::logging::warn(
            "pending_request_answer_denied",
            serde_json::json!({
                "client_instance_id": context.client_instance_id.as_str(),
                "task_id": task_id.as_str(),
                "reason_code": "task_outside_client_projects",
            }),
        );
        Some(ResponseOutcome::UnauthorizedResponder {
            request_id: request_id.clone(),
            responder: context.client_instance_id.clone(),
        })
    }

    fn publish_request_resolution(
        &mut self,
        scope: PendingRequestScope,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        match scope {
            PendingRequestScope::Task { task_id } => {
                self.publish_task_snapshot_replaced(&task_id, now)
            }
            // A Client-scoped request is a server request on that client's wire; nothing renders
            // it as state, so its resolution publishes nothing.
            PendingRequestScope::Client { .. } => Vec::new(),
        }
    }

    fn publish_task_snapshot_replaced(
        &mut self,
        task_id: &TaskId,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let requests = self.server_requests.pending_for_task(task_id);
        self.record_published_task_requests(task_id, &requests, "request_resolution");
        self.publish_task_payload(
            AppServerEventPayload::TaskRequestsUpdated {
                task_id: task_id.clone(),
                requests,
            },
            now,
        )
    }

    #[cfg(test)]
    fn eligible_deliveries_for_request(&self, draft: &ServerRequestDraft) -> Vec<Delivery> {
        match &draft.scope {
            PendingRequestScope::Client { client_instance_id } => self
                .client_hub
                .delivery_for(client_instance_id)
                .into_iter()
                .collect(),
            // Permission and Question requests reach clients as Task state, not as requests.
            PendingRequestScope::Task { .. }
                if crate::server_requests::is_task_state_request(&draft.method) =>
            {
                Vec::new()
            }
            PendingRequestScope::Task { task_id } => self
                .state_stream
                .subscribers_for_scope(
                    &openaide_app_server_protocol::state::SubscriptionScope::Task {
                        task_id: task_id.clone(),
                    },
                )
                .into_iter()
                .filter_map(|client_id| self.client_hub.delivery_for(&client_id))
                .collect(),
        }
    }
}

/// Logs only visibility transitions; a pending request republished with every streamed Task
/// update stays quiet.
fn log_task_requests_visibility(
    task_id: &TaskId,
    visible: bool,
    request_count: usize,
    source: &'static str,
) {
    crate::logging::info(
        "task_requests_visibility_changed",
        serde_json::json!({
            "task_id": task_id.as_str(),
            "visible": visible,
            "request_count": request_count,
            "source": source,
        }),
    );
}

fn server_request_response_error(outcome: ResponseOutcome) -> ProtocolError {
    match outcome {
        ResponseOutcome::InvalidResponse { message, .. } => ProtocolError {
            code: ProtocolErrorCode::ValidationFailed,
            message,
            recoverable: true,
            target: None,
        },
        ResponseOutcome::UnauthorizedResponder { .. } => ProtocolError {
            code: ProtocolErrorCode::Unauthorized,
            message: "This client cannot answer that permission request.".to_string(),
            recoverable: false,
            target: None,
        },
        ResponseOutcome::AlreadyResolved { .. }
        | ResponseOutcome::UnknownRequest { .. }
        | ResponseOutcome::StaleRequest { .. }
        | ResponseOutcome::Interrupted { .. } => ProtocolError {
            code: ProtocolErrorCode::RequestAlreadyResolved,
            message: "Permission request is no longer answerable.".to_string(),
            recoverable: false,
            target: None,
        },
        ResponseOutcome::Accepted { .. } => ProtocolError {
            code: ProtocolErrorCode::Internal,
            message: "server request response was already accepted".to_string(),
            recoverable: false,
            target: None,
        },
    }
}
