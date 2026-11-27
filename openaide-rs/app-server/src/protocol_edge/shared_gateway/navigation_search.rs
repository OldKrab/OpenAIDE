//! `taskNavigation/search` outside the protocol lock: scanning Chat text of many Tasks
//! would otherwise stall every other client's requests and events.

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::super::InboundProtocolMessage;
use super::{Handled, SharedRpcGateway};

impl SharedRpcGateway {
    /// Checks the caller and params under the lock, searches without it, then answers
    /// only if the connection still belongs to the client that asked.
    pub(super) fn handle_navigation_search_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        let prepared = {
            let mut gateway = self.lock();
            // Keep routing's initialization/shutdown rejection and lease activity semantics.
            if gateway.update_shutdown.is_some()
                || gateway
                    .client_hub
                    .context_for_connection(&connection_id)
                    .is_none()
            {
                let outcome = gateway.handle_inbound(connection_id, message, now);
                return (gateway, outcome);
            }
            if let Some(client) = gateway
                .client_hub
                .observe_connection_activity(&connection_id, now)
            {
                gateway.attachments.keep_alive_for_client(&client);
            }
            let InboundProtocolMessage::ClientRequest {
                id, params, meta, ..
            } = &message
            else {
                unreachable!("the search path only handles client requests")
            };
            match gateway.prepare_navigation_search(&connection_id, id, params.clone(), meta) {
                Ok(prepared) => prepared,
                Err(outcome) => return (gateway, *outcome),
            }
        };
        let InboundProtocolMessage::ClientRequest {
            id, method, meta, ..
        } = message
        else {
            unreachable!("the search path only handles client requests")
        };
        let owner = prepared.owner.clone();
        let result = prepared.run();
        let mut gateway = self.lock();
        // A reconnect or detach during the scan must not answer a new owner.
        if !gateway
            .client_hub
            .context_for_connection(&connection_id)
            .is_some_and(|client| client.client_instance_id == owner)
        {
            let outcome = gateway.error(
                connection_id,
                id,
                meta,
                super::super::responses::not_initialized(method),
            );
            return (gateway, outcome);
        }
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        let outcome = gateway.answer_navigation_search(connection_id, id, meta, result);
        (gateway, outcome)
    }
}
