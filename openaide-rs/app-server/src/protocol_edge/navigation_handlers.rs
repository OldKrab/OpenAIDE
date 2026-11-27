//! `navigation/*` requests and the one place Navigation frames are published.

use std::time::Instant;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
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

use super::{event_deliveries, responses, GatewayEventDelivery, GatewayOutcome, RpcGateway};

/// Bounds the flush loop: discovery can mark Navigation dirty while it is being asked to
/// deepen, and that mark is picked up by the next flush rather than spinning here.
const MAX_FLUSH_PASSES: usize = 3;

impl RpcGateway {
    /// Recomputes dirty Navigation once and publishes every affected view's frame, in one batch
    /// under the protocol lock. Returns what was published; it is also in the outbox.
    pub(crate) fn flush_navigation(&mut self, now: AppServerTime) -> Vec<GatewayEventDelivery> {
        let published = self.flush_navigation_unqueued(now);
        self.outbox.push_events(published.clone());
        published
    }

    /// Like `flush_navigation`, for a handler whose outcome already carries these events to the
    /// outbox ahead of its reply.
    fn flush_navigation_unqueued(&mut self, now: AppServerTime) -> Vec<GatewayEventDelivery> {
        let mut published = Vec::new();
        for _ in 0..MAX_FLUSH_PASSES {
            if !self.navigation.is_dirty() {
                break;
            }
            let flushed = self.navigation.flush();
            published.extend(self.publish_navigation_flush(flushed, now));
        }
        published
    }

    fn publish_navigation_flush(
        &mut self,
        flushed: FlushedFrames,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        for (project_id, _target) in &flushed.deepen {
            self.agent_list_sessions
                .request_native_session_discovery_deepening(project_id.as_str());
        }
        let mut events = Vec::new();
        for (client, frame) in flushed.frames {
            events.extend(self.publish_navigation_frame(&client, frame, now));
        }
        events
    }

    /// Sends a frame through the client's own subscription chain, so a missed frame shows as a
    /// cursor gap exactly like any other event.
    fn publish_navigation_frame(
        &mut self,
        client: &ClientInstanceId,
        frame: NavigationFrame,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let client_hub = self.client_hub.clone();
        event_deliveries(self.state_stream.publish_committed(
            EventScope::Client {
                state_root_id: self.state_stream.state_root_id().clone(),
                client_instance_id: client.clone(),
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
        // Bring every existing view current first, so this subscription's baseline cursor
        // orders after the frames those views were owed.
        let mut events = self.flush_navigation_unqueued(now);
        let section = params.section;
        let scope = SubscriptionScope::TaskNavigation { section };
        let newly_subscribed = !self.state_stream.is_subscribed(&ctx, &scope);
        let cursor = self
            .state_stream
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
                ..Default::default()
            });
        events.extend(self.publish_navigation_flush(flushed, now));
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
            NavigationSubscribeResult { cursor, frame: own },
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
        let mut events = self.flush_navigation_unqueued(now);
        let flushed = match self
            .navigation
            .upsert_view(&ctx.client_instance_id, params, false)
        {
            Ok(flushed) => flushed,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        events.extend(self.publish_navigation_flush(flushed, now));
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
