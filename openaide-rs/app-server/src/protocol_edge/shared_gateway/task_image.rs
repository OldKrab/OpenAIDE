//! `task/image` outside the protocol lock (ADR-0084): decoding and shrinking a large
//! screenshot takes long enough to stall every other client's requests and events.

use openaide_app_server_protocol::task::TaskImageResult;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::super::{GatewayOutcome, InboundProtocolMessage};
use super::SharedRpcGateway;

impl SharedRpcGateway {
    /// Checks access and reads the stored bytes under the lock, makes the preview
    /// without it, then answers only if the connection still belongs to the client
    /// that asked.
    pub(super) fn handle_task_image_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> GatewayOutcome {
        let prepared = {
            let mut gateway = self.lock();
            // Keep routing's initialization/shutdown rejection and lease activity semantics.
            if gateway.update_shutdown.is_some()
                || gateway
                    .client_hub
                    .context_for_connection(&connection_id)
                    .is_none()
            {
                return gateway.handle_inbound(connection_id, message, now);
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
                unreachable!("the task/image path only handles client requests")
            };
            match gateway.prepare_task_image(&connection_id, id, params.clone(), meta) {
                Ok(prepared) => prepared,
                Err(outcome) => return *outcome,
            }
        };
        let InboundProtocolMessage::ClientRequest {
            id, method, meta, ..
        } = message
        else {
            unreachable!("the task/image path only handles client requests")
        };
        let owner = prepared.owner.clone();
        let result = prepared.run();
        let mut gateway = self.lock();
        // A reconnect or detach while shrinking must not answer a new owner.
        if !gateway
            .client_hub
            .context_for_connection(&connection_id)
            .is_some_and(|client| client.client_instance_id == owner)
        {
            return gateway.error(
                connection_id,
                id,
                meta,
                super::super::responses::not_initialized(method),
            );
        }
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        gateway.result::<TaskImageResult>(connection_id, id, meta, result)
    }
}
