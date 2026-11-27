//! Requests that run slow work outside the protocol lock. Each takes the lock to prepare, runs
//! the work unlocked, and takes the lock again to finish; it returns the final guard with the
//! outcome, so the outcome joins the outbox before any later publication can.

use std::sync::Arc;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::super::InboundProtocolMessage;
use super::{Handled, SharedRpcGateway};

impl SharedRpcGateway {
    pub(super) fn handle_project_files_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        let read = {
            let mut gateway = self.lock();
            if gateway.update_shutdown.is_some()
                || gateway
                    .client_hub
                    .context_for_connection(&connection_id)
                    .is_none()
            {
                let outcome = gateway.handle_inbound(connection_id, message, now);
                return (gateway, outcome);
            }
            gateway
                .client_hub
                .observe_connection_activity(&connection_id, now);
            let InboundProtocolMessage::ClientRequest {
                id,
                method,
                params,
                meta,
                ..
            } = &message
            else {
                unreachable!()
            };
            match gateway.prepare_project_files(&connection_id, method, params.clone()) {
                Ok(read) => read,
                Err(error) => {
                    let outcome = gateway.error(connection_id, id.clone(), meta.clone(), error);
                    return (gateway, outcome);
                }
            }
        };
        let InboundProtocolMessage::ClientRequest {
            id, method, meta, ..
        } = message
        else {
            unreachable!()
        };
        let result = read.run(&id, &meta);
        let mut gateway = self.lock();
        if !gateway
            .client_hub
            .context_for_connection(&connection_id)
            .is_some_and(|client| client.client_instance_id == read.owner)
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
        let outcome = gateway.result(connection_id, id, meta, result);
        (gateway, outcome)
    }

    pub(super) fn handle_file_viewer_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        let read = {
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
                id,
                method,
                params,
                meta,
                ..
            } = &message
            else {
                unreachable!()
            };
            match gateway.prepare_file_viewer_read(&connection_id, method, params.clone()) {
                Ok(read) => read,
                Err(error) => {
                    let outcome = gateway.error(connection_id, id.clone(), meta.clone(), error);
                    return (gateway, outcome);
                }
            }
        };
        let InboundProtocolMessage::ClientRequest {
            id, method, meta, ..
        } = message
        else {
            unreachable!()
        };
        let snapshot = read.run(&id);
        let mut gateway = self.lock();
        // A reconnect or detach while decoding must not deliver old contents to a new owner.
        if !gateway
            .client_hub
            .context_for_connection(&connection_id)
            .is_some_and(|client| client.client_instance_id == read.owner)
        {
            gateway.file_viewer.release(&read.owner, &snapshot.handle);
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
        let outcome = gateway.result(connection_id, id, meta, snapshot);
        (gateway, outcome)
    }

    pub(super) fn handle_agent_authenticate_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        let InboundProtocolMessage::ClientRequest {
            id, params, meta, ..
        } = message
        else {
            unreachable!("authenticate path only handles client requests");
        };
        let (params, workflow) = {
            let mut gateway = self.lock();
            match gateway.prepare_agent_authenticate(
                connection_id.clone(),
                id.clone(),
                params,
                meta.clone(),
                now,
            ) {
                Ok(prepared) => prepared,
                Err(outcome) => {
                    gateway
                        .client_hub
                        .observe_connection_activity(&connection_id, completion_clock());
                    return (gateway, *outcome);
                }
            }
        };
        let secret_resolver =
            if params.secret_env.is_empty() || !connection_id.as_str().starts_with("local-http:") {
                None
            } else {
                self.client_secret_resolver(&connection_id)
            };
        let terminal_runner = self.client_auth_terminal(&connection_id);
        let result = workflow.authenticate_with_client(params, secret_resolver, terminal_runner);
        let mut gateway = self.lock();
        let outcome =
            gateway.finish_agent_authenticate(connection_id.clone(), id, meta, now, result);
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        (gateway, outcome)
    }

    fn client_secret_resolver(
        &self,
        connection_id: &ConnectionId,
    ) -> Option<Arc<dyn crate::agent::AgentSecretResolver>> {
        let gateway = self.lock();
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

    fn client_auth_terminal(
        &self,
        connection_id: &ConnectionId,
    ) -> Option<Arc<dyn crate::agent::auth_terminal::AuthTerminalRunner>> {
        if !connection_id.as_str().starts_with("local-http:") {
            return None;
        }
        let gateway = self.lock();
        let context = gateway.client_hub.context_for_connection(connection_id)?;
        let delivery = gateway
            .client_hub
            .delivery_for(&context.client_instance_id)?;
        Some(Arc::new(
            crate::agent::auth_terminal::ClientAuthTerminal::new(
                gateway.server_requests.clone(),
                context.client_instance_id,
                delivery,
            ),
        ))
    }

    pub(super) fn handle_agent_logout_without_protocol_lock(
        &self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
        completion_clock: impl FnOnce() -> AppServerTime,
    ) -> Handled<'_> {
        let InboundProtocolMessage::ClientRequest {
            id, params, meta, ..
        } = message
        else {
            unreachable!("logout path only handles client requests");
        };
        let mut gateway = self.lock();
        let (params, workflow) = match gateway.prepare_agent_logout(
            connection_id.clone(),
            id.clone(),
            params,
            meta.clone(),
            now,
        ) {
            Ok(prepared) => prepared,
            Err(outcome) => {
                gateway
                    .client_hub
                    .observe_connection_activity(&connection_id, completion_clock());
                return (gateway, *outcome);
            }
        };
        drop(gateway);
        let result = workflow.logout(params);
        let mut gateway = self.lock();
        let outcome = gateway.finish_agent_logout(connection_id.clone(), id, meta, now, result);
        gateway
            .client_hub
            .observe_connection_activity(&connection_id, completion_clock());
        (gateway, outcome)
    }
}
