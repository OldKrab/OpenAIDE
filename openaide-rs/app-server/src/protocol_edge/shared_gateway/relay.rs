//! Own Relay calls outside the protocol lock (ADR-0085): each one asks the relay
//! over the network and may wait seconds, which would stall every other client.

use std::time::Instant;

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::logging;

use super::super::device_handlers::relay_request;
use super::super::{responses, InboundProtocolMessage};
use super::{Handled, SharedRpcGateway};

impl SharedRpcGateway {
    /// Checks the client under the lock, calls the relay without it, then answers
    /// only if the connection still belongs to the client that asked.
    pub(super) fn handle_relay_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        let (devices, owner) = {
            let mut gateway = self.lock();
            // Keep routing's initialization/shutdown rejection and lease activity semantics.
            let owner = gateway
                .client_hub
                .context_for_connection(&connection_id)
                .map(|client| client.client_instance_id);
            let Some(owner) = owner.filter(|_| gateway.update_shutdown.is_none()) else {
                let outcome = gateway.handle_inbound(connection_id, message, now);
                return (gateway, outcome);
            };
            if let Some(client) = gateway
                .client_hub
                .observe_connection_activity(&connection_id, now)
            {
                gateway.attachments.keep_alive_for_client(&client);
            }
            (gateway.remote_devices.clone(), owner)
        };
        let InboundProtocolMessage::ClientRequest {
            id,
            method,
            params,
            meta,
            ..
        } = message
        else {
            unreachable!("the relay path only handles client requests")
        };
        let started = Instant::now();
        let answer = relay_request(devices.as_ref(), &method, params);
        logging::info(
            "remote_relay_request_completed",
            serde_json::json!({
                "connection_id": connection_id.as_str(),
                "method": method,
                "outcome": if answer.is_ok() { "succeeded" } else { "failed" },
                "error_code": answer.as_ref().err().map(|error| error.code),
                "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            }),
        );
        let mut gateway = self.lock();
        // A reconnect or detach while the relay answered must not answer a new owner.
        if !gateway
            .client_hub
            .context_for_connection(&connection_id)
            .is_some_and(|client| client.client_instance_id == owner)
        {
            let outcome =
                gateway.error(connection_id, id, meta, responses::not_initialized(method));
            return (gateway, outcome);
        }
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        let outcome = gateway.relay_answer(connection_id, id, &method, meta, answer);
        (gateway, outcome)
    }
}
