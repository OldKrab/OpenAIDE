use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::envelopes::{ErrorEnvelope, ResponseMeta};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::ids::{RequestId, TaskId};
use openaide_app_server_protocol::snapshot::{PendingRequestScope, TaskSnapshot};
use openaide_app_server_protocol::state::SubscriptionScope;

use crate::app_lifecycle::{ShutdownCompletion, ShutdownRequestOutcome};
#[cfg(test)]
use crate::client_lifecycle::Delivery;
use crate::client_lifecycle::{AppServerTime, ClientContext, ClientExpiryOutcome, ConnectionId};
use crate::protocol::errors::RuntimeError;
use crate::protocol_edge::{
    event_deliveries, responses, GatewayEventDelivery, GatewayOutcome, GatewayResponse,
    IdleShutdownDecision, RpcGateway,
};
use crate::server_requests::ServerRequestDelivery;
#[cfg(test)]
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};
use crate::server_requests::{ResponderScope, ResponseOutcome, ServerRequestAnswer};

impl RpcGateway {
    pub fn shutdown(&mut self) -> Result<ShutdownCompletion, RuntimeError> {
        if matches!(
            self.lifecycle.request_shutdown(),
            ShutdownRequestOutcome::AlreadyStopping
        ) {
            return Ok(ShutdownCompletion::CleanRelease);
        }
        match self.shutdown.shutdown() {
            Ok(()) => Ok(self.lifecycle.complete_shutdown(true)),
            Err(error) => {
                let _ = self.lifecycle.complete_shutdown(false);
                Err(error)
            }
        }
    }

    pub(crate) fn idle_shutdown_decision(&self) -> Result<IdleShutdownDecision, RuntimeError> {
        let initialized_clients = self.client_hub.has_initialized_clients();
        let blockers = self.shutdown.shutdown_blockers()?;
        if !initialized_clients && blockers.is_empty() {
            return Ok(IdleShutdownDecision::ShutdownNow);
        }
        Ok(IdleShutdownDecision::KeepRunning {
            initialized_clients,
            blockers,
        })
    }

    pub fn handle_transport_closed(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> crate::client_lifecycle::TransportClosedOutcome {
        let outcome = self.client_hub.observe_transport_closed(connection_id, now);
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

    pub fn expire_client_after_reconnect_grace(
        &mut self,
        client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        now: AppServerTime,
    ) -> ClientExpiryOutcome {
        let outcome = self.client_hub.expire_after_grace(client_instance_id, now);
        if let ClientExpiryOutcome::Expired { last_client, .. } = &outcome {
            self.server_requests
                .observe_client_expired(client_instance_id, now);
            self.remove_expired_client_workspace_roots(client_instance_id, now);
            if *last_client {
                self.lifecycle.observe_last_client_expired();
            }
        }
        outcome
    }

    pub fn expire_inactive_clients(&mut self, now: AppServerTime) -> Vec<ClientExpiryOutcome> {
        let batch = self.client_hub.expire_inactive_clients(now);
        let mut projects_changed = false;
        for client_instance_id in &batch.expired {
            self.server_requests
                .observe_client_expired(client_instance_id, now);
            projects_changed |= self
                .project_roots
                .remove_client_workspace_roots(client_instance_id);
        }
        if projects_changed {
            self.queue_project_collection_update(now);
        }
        if batch.last_client_expired {
            self.lifecycle.observe_last_client_expired();
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

    fn remove_expired_client_workspace_roots(
        &mut self,
        client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        now: AppServerTime,
    ) {
        if self
            .project_roots
            .remove_client_workspace_roots(client_instance_id)
        {
            self.queue_project_collection_update(now);
        }
    }

    fn queue_project_collection_update(&mut self, now: AppServerTime) {
        if let Some(events) = self.publish_project_collection_update(now) {
            self.pending_event_deliveries.extend(events);
        }
    }

    pub(crate) fn drain_event_deliveries_for_connection(
        &mut self,
        connection_id: &ConnectionId,
    ) -> Vec<GatewayEventDelivery> {
        let mut drained = Vec::new();
        self.pending_event_deliveries.retain(|event| {
            if &event.delivery.connection_id == connection_id {
                drained.push(event.clone());
                false
            } else {
                true
            }
        });
        drained
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
            &self.responder_scopes(&context),
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
        let outcome = self.server_requests.handle_response_from_scopes(
            context.client_instance_id.clone(),
            RequestId::from(request_id.clone()),
            answer,
            &self.responder_scopes(&context),
            now,
        );
        let ResponseOutcome::Accepted { scope, .. } = outcome else {
            return GatewayOutcome::Respond {
                connection_id,
                id: request_id,
                response: GatewayResponse::Error(ErrorEnvelope::new(
                    server_request_response_error(outcome),
                    ResponseMeta::default(),
                )),
                events: Vec::new(),
                server_requests: Vec::new(),
            };
        };

        let events = self.publish_request_resolution(&context, scope, now);
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
        &self,
        snapshot: &mut openaide_app_server_protocol::state::SubscriptionSnapshot,
    ) {
        if let openaide_app_server_protocol::state::SubscriptionSnapshot::Task { task } = snapshot {
            *task = self.task_with_pending_requests(task.clone());
        }
    }

    pub(super) fn task_with_pending_requests(&self, mut task: TaskSnapshot) -> TaskSnapshot {
        task.pending_requests = self.server_requests.pending_for_task(&task.task.task_id);
        task
    }

    pub(super) fn responder_scopes(&self, context: &ClientContext) -> Vec<ResponderScope> {
        let mut scopes = vec![ResponderScope::Client(context.client_instance_id.clone())];
        scopes.extend(
            self.state_stream
                .subscriptions_for_client(&context.client_instance_id)
                .into_iter()
                .filter_map(|scope| match scope {
                    SubscriptionScope::Task { task_id } => Some(ResponderScope::Task(task_id)),
                    _ => None,
                }),
        );
        scopes
    }

    pub(super) fn result_with_server_requests<T: serde::Serialize>(
        &self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        result: T,
        server_requests: Vec<ServerRequestDelivery>,
    ) -> GatewayOutcome {
        match responses::result(connection_id, id, meta, result) {
            GatewayOutcome::Respond {
                connection_id,
                id,
                response,
                events,
                ..
            } => GatewayOutcome::Respond {
                connection_id,
                id,
                response,
                events,
                server_requests,
            },
            GatewayOutcome::Noop => GatewayOutcome::Noop,
        }
    }

    fn publish_request_resolution(
        &mut self,
        context: &ClientContext,
        scope: PendingRequestScope,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        match scope {
            PendingRequestScope::Task { task_id } => {
                self.publish_task_snapshot_replaced(&task_id, now)
            }
            PendingRequestScope::Client { client_instance_id }
                if client_instance_id == context.client_instance_id =>
            {
                self.publish_client_snapshot_replaced(context, now)
            }
            PendingRequestScope::Client { .. } => Vec::new(),
        }
    }

    fn publish_task_snapshot_replaced(
        &mut self,
        task_id: &TaskId,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let Ok(task) = self.task_snapshots.open(task_id) else {
            return Vec::new();
        };
        let task = self.task_with_pending_requests(task);
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::Task {
                state_root_id: self.state_stream.state_root_id().clone(),
                task_id: task_id.clone(),
            },
            AppServerEventPayload::TaskSnapshotUpdated { task },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    fn publish_client_snapshot_replaced(
        &mut self,
        context: &ClientContext,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let token = self
            .state_stream
            .read_token_for_client(&context.client_instance_id);
        let Ok(mut snapshot) =
            self.snapshots
                .client_snapshot(context, context.requested_surface.clone(), &token)
        else {
            return Vec::new();
        };
        snapshot.pending_requests = self
            .server_requests
            .pending_for_client(&context.client_instance_id);
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::Client {
                state_root_id: self.state_stream.state_root_id().clone(),
                client_instance_id: context.client_instance_id.clone(),
            },
            AppServerEventPayload::SnapshotReplaced { snapshot },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    #[cfg(test)]
    fn eligible_deliveries_for_request(&self, draft: &ServerRequestDraft) -> Vec<Delivery> {
        match &draft.scope {
            PendingRequestScope::Client { client_instance_id } => self
                .client_hub
                .delivery_for(client_instance_id)
                .into_iter()
                .collect(),
            PendingRequestScope::Task { .. }
                if matches!(
                    draft.method.as_str(),
                    openaide_app_server_protocol::server_requests::PERMISSION_REQUEST
                        | openaide_app_server_protocol::server_requests::QUESTION_REQUEST
                ) =>
            {
                self.client_hub.deliveries_supporting(&draft.method)
            }
            PendingRequestScope::Task { task_id } => self
                .state_stream
                .subscribers_for_scope(&SubscriptionScope::Task {
                    task_id: task_id.clone(),
                })
                .into_iter()
                .filter_map(|client_id| self.client_hub.delivery_for(&client_id))
                .collect(),
        }
    }
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
