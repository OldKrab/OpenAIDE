use std::collections::HashMap;

use openaide_app_server_protocol::client::{ClientCapabilities, RequestedSurface, ShellDescriptor};
use openaide_app_server_protocol::ids::ClientInstanceId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionId(String);

impl ConnectionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AppServerTime(pub u64);

impl AppServerTime {
    pub fn now() -> Self {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self(millis.try_into().unwrap_or(u64::MAX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientContext {
    pub client_instance_id: ClientInstanceId,
    pub connection_id: ConnectionId,
    pub shell: ShellDescriptor,
    pub requested_surface: RequestedSurface,
    pub capabilities: ClientCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitializeClientOutcome {
    NewClient { context: ClientContext },
    ReattachedClient { context: ClientContext },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportClosedOutcome {
    EnteredReconnectGrace {
        client_instance_id: ClientInstanceId,
        expires_at: AppServerTime,
    },
    UnknownConnection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientExpiryOutcome {
    Expired {
        client_instance_id: ClientInstanceId,
        last_client: bool,
    },
    StillInGrace {
        expires_at: AppServerTime,
    },
    ClientConnected,
    UnknownClient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientExpiryBatch {
    pub expired: Vec<ClientInstanceId>,
    pub last_client_expired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityChangeOutcome {
    Updated { context: ClientContext },
    UnknownClient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetachOutcome {
    Detached {
        client_instance_id: ClientInstanceId,
        last_client: bool,
    },
    UnknownClient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub client_instance_id: ClientInstanceId,
    pub connection_id: ConnectionId,
    pub request_capabilities: Vec<RequestCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestCapability {
    Permission,
    Question,
}

impl Delivery {
    pub fn new(client_instance_id: ClientInstanceId, connection_id: ConnectionId) -> Self {
        Self {
            client_instance_id,
            connection_id,
            request_capabilities: Vec::new(),
        }
    }

    pub fn with_request_capabilities(mut self, capabilities: Vec<RequestCapability>) -> Self {
        self.request_capabilities = capabilities;
        self
    }

    pub fn supports_method(&self, method: &str) -> bool {
        let required = match method {
            openaide_app_server_protocol::server_requests::PERMISSION_REQUEST => {
                RequestCapability::Permission
            }
            openaide_app_server_protocol::server_requests::QUESTION_REQUEST => {
                RequestCapability::Question
            }
            _ => return true,
        };
        self.request_capabilities.contains(&required)
    }
}

#[derive(Debug, Clone)]
struct ClientRecord {
    context: ClientContext,
    reconnect_expires_at: Option<AppServerTime>,
    liveness_expires_at: AppServerTime,
}

#[derive(Debug, Clone)]
pub struct ClientHub {
    reconnect_grace_ms: u64,
    clients: HashMap<ClientInstanceId, ClientRecord>,
    connections: HashMap<ConnectionId, ClientInstanceId>,
}

impl ClientHub {
    pub fn new(reconnect_grace_ms: u64) -> Self {
        Self {
            reconnect_grace_ms,
            clients: HashMap::new(),
            connections: HashMap::new(),
        }
    }

    pub fn initialize(
        &mut self,
        connection_id: ConnectionId,
        params: openaide_app_server_protocol::client::InitializeParams,
        now: AppServerTime,
    ) -> InitializeClientOutcome {
        let context = ClientContext {
            client_instance_id: params.client_instance_id,
            connection_id: connection_id.clone(),
            shell: params.shell,
            requested_surface: params.requested_surface,
            capabilities: params.capabilities,
        };
        let was_present = self.clients.contains_key(&context.client_instance_id);
        self.connections
            .retain(|_, client_instance_id| client_instance_id != &context.client_instance_id);
        self.connections
            .insert(connection_id, context.client_instance_id.clone());
        self.clients.insert(
            context.client_instance_id.clone(),
            ClientRecord {
                context: context.clone(),
                reconnect_expires_at: None,
                liveness_expires_at: AppServerTime(now.0 + self.reconnect_grace_ms),
            },
        );

        if was_present {
            InitializeClientOutcome::ReattachedClient { context }
        } else {
            InitializeClientOutcome::NewClient { context }
        }
    }

    pub fn update_capabilities(
        &mut self,
        client_instance_id: &ClientInstanceId,
        capabilities: ClientCapabilities,
    ) -> CapabilityChangeOutcome {
        let Some(record) = self.clients.get_mut(client_instance_id) else {
            return CapabilityChangeOutcome::UnknownClient;
        };
        record.context.capabilities = capabilities;
        CapabilityChangeOutcome::Updated {
            context: record.context.clone(),
        }
    }

    pub fn observe_transport_closed(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> TransportClosedOutcome {
        let Some(client_instance_id) = self.connections.remove(connection_id) else {
            return TransportClosedOutcome::UnknownConnection;
        };
        let expires_at = AppServerTime(now.0 + self.reconnect_grace_ms);
        if let Some(record) = self.clients.get_mut(&client_instance_id) {
            record.reconnect_expires_at = Some(expires_at);
        }
        TransportClosedOutcome::EnteredReconnectGrace {
            client_instance_id,
            expires_at,
        }
    }

    pub fn observe_connection_activity(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> Option<ClientInstanceId> {
        let client_instance_id = self.connections.get(connection_id)?.clone();
        let record = self.clients.get_mut(&client_instance_id)?;
        record.liveness_expires_at = AppServerTime(now.0 + self.reconnect_grace_ms);
        Some(client_instance_id)
    }

    pub fn expire_after_grace(
        &mut self,
        client_instance_id: &ClientInstanceId,
        now: AppServerTime,
    ) -> ClientExpiryOutcome {
        let Some(record) = self.clients.get(client_instance_id) else {
            return ClientExpiryOutcome::UnknownClient;
        };
        let Some(expires_at) = record.reconnect_expires_at else {
            return ClientExpiryOutcome::ClientConnected;
        };
        if now < expires_at {
            return ClientExpiryOutcome::StillInGrace { expires_at };
        }

        let client_instance_id = client_instance_id.clone();
        self.clients.remove(&client_instance_id);
        self.connections
            .retain(|_, value| value != &client_instance_id);
        ClientExpiryOutcome::Expired {
            client_instance_id,
            last_client: self.clients.is_empty(),
        }
    }

    pub fn expire_inactive_clients(&mut self, now: AppServerTime) -> ClientExpiryBatch {
        let expired = self
            .clients
            .iter()
            .filter_map(|(client_instance_id, record)| {
                let reconnect_expired = record
                    .reconnect_expires_at
                    .map(|expires_at| now >= expires_at)
                    .unwrap_or(false);
                let liveness_expired = now >= record.liveness_expires_at;
                (reconnect_expired || liveness_expired).then(|| client_instance_id.clone())
            })
            .collect::<Vec<_>>();

        for client_instance_id in &expired {
            self.clients.remove(client_instance_id);
            self.connections
                .retain(|_, value| value != client_instance_id);
        }

        ClientExpiryBatch {
            last_client_expired: !expired.is_empty() && self.clients.is_empty(),
            expired,
        }
    }

    pub fn detach(&mut self, client_instance_id: &ClientInstanceId) -> DetachOutcome {
        if self.clients.remove(client_instance_id).is_none() {
            return DetachOutcome::UnknownClient;
        }
        self.connections
            .retain(|_, value| value != client_instance_id);
        DetachOutcome::Detached {
            client_instance_id: client_instance_id.clone(),
            last_client: self.clients.is_empty(),
        }
    }

    pub fn context_for_connection(&self, connection_id: &ConnectionId) -> Option<ClientContext> {
        let client_instance_id = self.connections.get(connection_id)?;
        self.clients
            .get(client_instance_id)
            .map(|record| record.context.clone())
    }

    pub fn has_initialized_clients(&self) -> bool {
        !self.clients.is_empty()
    }

    pub fn client_by_instance(
        &self,
        client_instance_id: &ClientInstanceId,
    ) -> Option<ClientContext> {
        self.clients
            .get(client_instance_id)
            .map(|record| record.context.clone())
    }

    pub fn delivery_for(&self, client_instance_id: &ClientInstanceId) -> Option<Delivery> {
        let context = &self.clients.get(client_instance_id)?.context;
        Some(
            Delivery::new(client_instance_id.clone(), context.connection_id.clone())
                .with_request_capabilities(request_capabilities(&context.capabilities)),
        )
    }

    /// Returns connected clients that declared support for an App Server request method.
    pub fn deliveries_supporting(&self, method: &str) -> Vec<Delivery> {
        self.clients
            .keys()
            .filter_map(|client_instance_id| self.delivery_for(client_instance_id))
            .filter(|delivery| delivery.supports_method(method))
            .collect()
    }
}

fn request_capabilities(capabilities: &ClientCapabilities) -> Vec<RequestCapability> {
    use openaide_app_server_protocol::client::ClientProtocolCapability;

    let mut result = Vec::new();
    if capabilities
        .protocol
        .contains(&ClientProtocolCapability::PermissionResponses)
    {
        result.push(RequestCapability::Permission);
    }
    if capabilities
        .protocol
        .contains(&ClientProtocolCapability::QuestionResponses)
    {
        result.push(RequestCapability::Question);
    }
    result
}

#[cfg(test)]
mod tests;
