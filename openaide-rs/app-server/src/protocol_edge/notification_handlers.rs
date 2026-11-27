//! Client-to-server notifications (ADR-0086, "Two primitives"). A notification is never
//! answered; whatever its handler publishes joins the outbound sequences like any other change.

use openaide_app_server_protocol::client::ClientSecretKeysChangedParams;
use openaide_app_server_protocol::methods::CLIENT_SECRET_KEYS_CHANGED;
use serde_json::{json, Value};

use crate::client_lifecycle::{AppServerTime, ClientContext, ConnectionId};
use crate::logging;

use super::{GatewayOutcome, RpcGateway};

/// Handles one notification from an initialized client. Its outcome's events and server
/// requests are queued; a reply in it is dropped.
type ClientNotificationHandler =
    fn(&mut RpcGateway, &ClientContext, Value, AppServerTime) -> GatewayOutcome;

/// Register a client notification by adding `(METHOD, handler)` here.
const CLIENT_NOTIFICATION_HANDLERS: &[(&str, ClientNotificationHandler)] =
    &[(CLIENT_SECRET_KEYS_CHANGED, handle_secret_keys_changed)];

/// Replaces the secure-storage keys the client holds; `secret/read` is routed by them.
fn handle_secret_keys_changed(
    gateway: &mut RpcGateway,
    context: &ClientContext,
    params: Value,
    _now: AppServerTime,
) -> GatewayOutcome {
    match serde_json::from_value::<ClientSecretKeysChangedParams>(params) {
        Ok(params) => gateway
            .server_requests
            .declare_secret_keys(context.client_instance_id.clone(), params.key_ids),
        Err(_) => log_rejected(
            &context.connection_id,
            CLIENT_SECRET_KEYS_CHANGED,
            "invalid_params",
        ),
    }
    GatewayOutcome::Noop
}

fn client_notification_handler(method: &str) -> Option<ClientNotificationHandler> {
    CLIENT_NOTIFICATION_HANDLERS
        .iter()
        .find(|(registered, _)| *registered == method)
        .map(|(_, handler)| *handler)
}

impl RpcGateway {
    pub(super) fn handle_client_notification(
        &mut self,
        connection_id: ConnectionId,
        method: String,
        params: Value,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            log_rejected(&connection_id, &method, "not_initialized");
            return GatewayOutcome::Noop;
        };
        let Some(handler) = client_notification_handler(&method) else {
            log_rejected(&connection_id, &method, "unknown_method");
            return GatewayOutcome::Noop;
        };
        self.client_hub
            .observe_connection_activity(&connection_id, now);
        handler(self, &context, params, now)
    }
}

/// The method is a protocol constant chosen by the client build, not user content.
fn log_rejected(connection_id: &ConnectionId, method: &str, reason_code: &'static str) {
    logging::warn(
        "client_notification_rejected",
        json!({
            "connection_id": connection_id.as_str(),
            "method": method,
            "reason_code": reason_code,
        }),
    );
}

#[cfg(test)]
#[path = "notification_handlers_tests.rs"]
mod tests;
