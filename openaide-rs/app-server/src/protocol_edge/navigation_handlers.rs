//! `navigation/*` requests and the one place Navigation frames are published.

use std::time::Instant;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::events::AppServerEventPayload;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::methods::{
    NAVIGATION_SUBSCRIBE, NAVIGATION_UNSUBSCRIBE, NAVIGATION_UPDATE,
};
use openaide_app_server_protocol::navigation::{
    NavigationFrame, NavigationSubscribeResult, NavigationUnsubscribeParams,
    NavigationUnsubscribeResult, NavigationUpdateResult, NavigationViewParams,
};
use openaide_app_server_protocol::state::SubscriptionScope;
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::navigation::FlushedFrames;
use crate::state_sync::EventAddress;

use super::{event_deliveries, responses, GatewayEventDelivery, GatewayOutcome, RpcGateway};

/// Bounds the flush loop: discovery can mark Navigation dirty while it is being asked to
/// deepen, and that mark is picked up by the next flush rather than spinning here.
const MAX_FLUSH_PASSES: usize = 3;

impl RpcGateway {
    /// Recomputes dirty Navigation once and publishes every affected view's frame, in one batch
    /// under the protocol lock. Returns what was published; it is also in the outbox.
    pub(crate) fn flush_navigation(&mut self, now: AppServerTime) -> Vec<GatewayEventDelivery> {
        let published = self.flush_navigation_unqueued(now, false);
        self.outbox.push_events(published.clone());
        published
    }

    /// Flushes dirty Navigation and queues the frames after the events `outcome` already carries,
    /// which are older frames the handler stamped. Appending keeps the outbox in revision order.
    pub(crate) fn append_navigation_flush(
        &mut self,
        outcome: GatewayOutcome,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let ahead_of_reply = matches!(outcome, GatewayOutcome::Respond { .. });
        let flushed = self.flush_navigation_unqueued(now, ahead_of_reply);
        match outcome {
            GatewayOutcome::Respond {
                connection_id,
                id,
                response,
                mut events,
                server_requests,
            } => {
                events.extend(flushed);
                GatewayOutcome::Respond {
                    connection_id,
                    id,
                    response,
                    events,
                    server_requests,
                }
            }
            GatewayOutcome::Noop => {
                self.outbox.push_events(flushed);
                GatewayOutcome::Noop
            }
        }
    }

    /// Like `flush_navigation`, for a handler whose outcome already carries these events to the
    /// outbox ahead of its reply.
    /// `ahead_of_reply` is whether the frames ride in front of a request's reply (recorded in
    /// the `navigation_frame_published` event).
    fn flush_navigation_unqueued(
        &mut self,
        now: AppServerTime,
        ahead_of_reply: bool,
    ) -> Vec<GatewayEventDelivery> {
        let mut published = Vec::new();
        for _ in 0..MAX_FLUSH_PASSES {
            if !self.navigation.is_dirty() {
                break;
            }
            let flushed = self.navigation.flush();
            published.extend(self.publish_navigation_flush(flushed, now, ahead_of_reply));
        }
        published
    }

    fn publish_navigation_flush(
        &mut self,
        flushed: FlushedFrames,
        now: AppServerTime,
        ahead_of_reply: bool,
    ) -> Vec<GatewayEventDelivery> {
        for (project_id, _target) in &flushed.deepen {
            self.agent_list_sessions
                .request_native_session_discovery_deepening(project_id.as_str());
        }
        let mut events = Vec::new();
        for (client, frame) in flushed.frames {
            events.extend(self.publish_navigation_frame(&client, frame, now, ahead_of_reply));
        }
        events
    }

    /// Sends a frame to the one client whose view it is.
    fn publish_navigation_frame(
        &mut self,
        client: &ClientInstanceId,
        frame: NavigationFrame,
        now: AppServerTime,
        ahead_of_reply: bool,
    ) -> Vec<GatewayEventDelivery> {
        log_navigation_frame_published(client, &frame, ahead_of_reply);
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish(
            EventAddress::Client {
                client: client.clone(),
                scope: SubscriptionScope::TaskNavigation {
                    section: frame.section,
                },
            },
            AppServerEventPayload::NavigationFrame { frame },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }

    pub(super) fn handle_navigation_subscribe(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let started_at = Instant::now();
        let params = match serde_json::from_value::<NavigationViewParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(ctx) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(NAVIGATION_SUBSCRIBE.to_string()),
            );
        };
        // Bring every existing view current first, so this subscription's baseline revision
        // orders after the frames those views were owed.
        let mut events = self.flush_navigation_unqueued(now, true);
        let section = params.section;
        let scope = SubscriptionScope::TaskNavigation { section };
        let newly_subscribed = !self.state_stream.is_subscribed(&ctx, &scope);
        self.state_stream
            .register_view_subscription(&ctx, scope.clone());
        let mut flushed = match self
            .navigation
            .upsert_view(&ctx.client_instance_id, params, true)
        {
            Ok(flushed) => flushed,
            Err(error) => {
                self.state_stream.unsubscribe(&ctx, scope, now);
                return self.error(connection_id, id, meta, error);
            }
        };
        let own = flushed
            .frames
            .iter()
            .position(|(client, frame)| {
                client == &ctx.client_instance_id && frame.section == section
            })
            .map(|index| flushed.frames.remove(index).1)
            .unwrap_or_else(|| NavigationFrame {
                section,
                revision: self.navigation.revision(),
                ..Default::default()
            });
        events.extend(self.publish_navigation_flush(flushed, now, true));
        crate::logging::info(
            "navigation_subscribed",
            serde_json::json!({
                "operation": "navigation_subscribe",
                "outcome": "completed",
                "section": format!("{section:?}"),
                "group_count": own.groups.len(),
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        // The baseline is immediate; discovery then deepens it asynchronously and each
        // change arrives as a frame. Re-subscribing an existing view does not restart it.
        if newly_subscribed {
            self.agent_list_sessions
                .request_native_session_catalog_refresh(
                    crate::tasks::product_api::NativeCatalogRefreshRequester::NavigationSubscribed,
                );
        }
        self.result_with_events::<NavigationSubscribeResult>(
            connection_id,
            id,
            meta,
            NavigationSubscribeResult { frame: own },
            events,
        )
    }

    pub(super) fn handle_navigation_update(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let started_at = Instant::now();
        let params = match serde_json::from_value::<NavigationViewParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(ctx) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(NAVIGATION_UPDATE.to_string()),
            );
        };
        let section = params.section;
        if !self
            .state_stream
            .is_subscribed(&ctx, &SubscriptionScope::TaskNavigation { section })
        {
            return self.error(
                connection_id,
                id,
                meta,
                openaide_app_server_protocol::errors::ProtocolError {
                    code: openaide_app_server_protocol::errors::ProtocolErrorCode::InvalidRequest,
                    message: "navigation/update needs an active navigation subscription"
                        .to_string(),
                    recoverable: true,
                    target: None,
                },
            );
        }
        let mut events = self.flush_navigation_unqueued(now, true);
        let flushed = match self
            .navigation
            .upsert_view(&ctx.client_instance_id, params, false)
        {
            Ok(flushed) => flushed,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        events.extend(self.publish_navigation_flush(flushed, now, true));
        crate::logging::info(
            "navigation_updated",
            serde_json::json!({
                "operation": "navigation_update",
                "outcome": "completed",
                "section": format!("{section:?}"),
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        self.result_with_events::<NavigationUpdateResult>(
            connection_id,
            id,
            meta,
            NavigationUpdateResult {},
            events,
        )
    }

    pub(super) fn handle_navigation_unsubscribe(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NavigationUnsubscribeParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(ctx) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(NAVIGATION_UNSUBSCRIBE.to_string()),
            );
        };
        self.state_stream.unsubscribe(
            &ctx,
            SubscriptionScope::TaskNavigation {
                section: params.section,
            },
            now,
        );
        self.navigation
            .remove_view(&ctx.client_instance_id, params.section);
        crate::logging::info(
            "navigation_unsubscribed",
            serde_json::json!({
                "operation": "navigation_unsubscribe",
                "outcome": "completed",
                "section": format!("{:?}", params.section),
            }),
        );
        self.result::<NavigationUnsubscribeResult>(
            connection_id,
            id,
            meta,
            NavigationUnsubscribeResult {},
        )
    }
}

/// Records the ordering facts of one published frame (metadata only: revision, counts, section,
/// the opaque client instance id). With the debug level on, a reordering like a delta overtaking
/// the frame that carried its rows is visible from the server log without a browser capture.
fn log_navigation_frame_published(
    client: &ClientInstanceId,
    frame: &NavigationFrame,
    ahead_of_reply: bool,
) {
    let row_body_count: usize = frame.groups.iter().map(|group| group.rows.len()).sum();
    let order_len: usize = frame.groups.iter().map(|group| group.order.len()).sum();
    crate::logging::debug(
        "navigation_frame_published",
        serde_json::json!({
            "client_instance_id": client.as_str(),
            "section": format!("{:?}", frame.section),
            "revision": frame.revision,
            "group_count": frame.groups.len(),
            "row_body_count": row_body_count,
            "order_len": order_len,
            "ahead_of_reply": ahead_of_reply,
        }),
    );
}
