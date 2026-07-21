use std::env;
use std::sync::mpsc;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::agent::acp::AcpAgentRuntime;
use crate::agent::catalog_store::AgentCatalogStore;
use crate::agent::{registry_handle::AgentRegistryHandle, AgentRuntime};
use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::projects::ConfiguredProjectRoots;
use crate::protocol::host::{HostBridge, HostRequest};
use crate::protocol_edge::{GatewayOutcome, InboundProtocolMessage, SharedRpcGateway};
use crate::storage::Store;
use crate::storage_runtime::StateRoot;
use crate::task_events::{TaskUpdate, TaskUpdateReceiver};
use crate::worktree_events::WorktreeUpdateReceiver;

mod factory;
#[cfg(test)]
mod tests;
pub(crate) mod wire;

use wire::{
    client_response, event_wire_messages, id_to_gateway_id, invalid_request, parse_error,
    serialize_message, server_request_wire_messages, wire_messages, WireMessage, WireRequest,
    WireRequestId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpHostRequestTransport {
    Stdio,
    Unavailable,
}

pub struct ProtocolEdgeStdioDispatcher {
    gateway: SharedRpcGateway,
    #[cfg(test)]
    attachment_runtime: crate::attachment_runtime::AttachmentRuntime,
    connection_id: ConnectionId,
    next_tick: u64,
    task_updates: Option<TaskUpdateReceiver>,
    worktree_updates: Option<WorktreeUpdateReceiver>,
    host_bridge: HostBridge,
    host_requests: Option<mpsc::Receiver<HostRequest>>,
    storage_fatal_events:
        Option<mpsc::Receiver<crate::storage::task_journal::TaskStorageFatalFailure>>,
}

impl ProtocolEdgeStdioDispatcher {
    pub fn new(state_root: StateRoot) -> Self {
        Self::try_new(state_root).expect("protocol edge dispatcher storage must open")
    }

    pub fn try_new(state_root: StateRoot) -> Result<Self, ProtocolEdgeStdioStartError> {
        Self::try_new_with_host_request_transport(state_root, AcpHostRequestTransport::Stdio)
    }

    pub fn try_new_with_host_request_transport(
        state_root: StateRoot,
        host_request_transport: AcpHostRequestTransport,
    ) -> Result<Self, ProtocolEdgeStdioStartError> {
        let store = Store::open(state_root.path().to_path_buf())?;
        let agent_registry =
            AgentRegistryHandle::new(AgentCatalogStore::new(store.clone()).registry()?);
        let (host_bridge, host_requests) = match host_request_transport {
            AcpHostRequestTransport::Stdio => {
                let (host_bridge, host_requests) = HostBridge::channel();
                (host_bridge, Some(host_requests))
            }
            AcpHostRequestTransport::Unavailable => (HostBridge::disabled(), None),
        };
        let acp_trace_state = crate::agent::acp_trace::AcpTraceState::from_env(state_root.path());
        let agent_runtime = Arc::new(
            AcpAgentRuntime::new_with_registry(agent_registry.clone(), host_bridge.clone())
                .with_trace_state(acp_trace_state.clone()),
        );
        Self::try_new_with_agent(
            state_root,
            store,
            agent_registry,
            agent_runtime,
            acp_trace_state,
            configured_project_roots_from_env(),
            (host_bridge, host_requests),
        )
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(state_root: StateRoot) -> Self {
        Self::new_for_test_with_configured_projects(state_root, ConfiguredProjectRoots::default())
    }

    #[cfg(test)]
    pub(crate) fn new_for_test_with_configured_projects(
        state_root: StateRoot,
        configured_projects: ConfiguredProjectRoots,
    ) -> Self {
        let store = Store::open(state_root.path().to_path_buf())
            .expect("protocol edge test dispatcher storage must open");
        Self::try_new_with_agent(
            state_root,
            store,
            AgentRegistryHandle::new(crate::agent::registry::AgentRegistry::default_built_ins()),
            Arc::new(crate::agent::mock::MockAgent),
            crate::agent::acp_trace::AcpTraceState::disabled(std::path::Path::new(".")),
            configured_projects,
            (HostBridge::disabled(), None),
        )
        .expect("protocol edge test dispatcher storage must open")
    }

    fn try_new_with_agent(
        state_root: StateRoot,
        store: Store,
        agent_registry: AgentRegistryHandle,
        agent_runtime: Arc<dyn AgentRuntime>,
        acp_trace_state: crate::agent::acp_trace::AcpTraceState,
        configured_projects: ConfiguredProjectRoots,
        host_transport: (HostBridge, Option<mpsc::Receiver<HostRequest>>),
    ) -> Result<Self, ProtocolEdgeStdioStartError> {
        let (host_bridge, host_requests) = host_transport;
        let output = factory::gateway(
            state_root,
            store,
            agent_registry,
            agent_runtime,
            acp_trace_state,
            configured_projects,
        )?;
        Ok(Self {
            gateway: SharedRpcGateway::new(output.gateway),
            #[cfg(test)]
            attachment_runtime: output.attachment_runtime,
            connection_id: ConnectionId::new("stdio"),
            next_tick: 1,
            task_updates: Some(output.task_updates),
            worktree_updates: Some(output.worktree_updates),
            host_bridge,
            host_requests,
            storage_fatal_events: Some(output.storage_fatal_events),
        })
    }

    pub fn take_task_updates(&mut self) -> Option<TaskUpdateReceiver> {
        self.task_updates.take()
    }

    pub fn take_worktree_updates(&mut self) -> Option<WorktreeUpdateReceiver> {
        self.worktree_updates.take()
    }

    pub fn handle_worktree_update(
        &mut self,
        repository: openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot,
    ) -> Vec<String> {
        let now = self.next_time();
        let events = self
            .gateway
            .publish_worktree_repository_update(repository, now);
        event_wire_messages(self.connection_id.clone(), events)
            .into_iter()
            .map(serialize_message)
            .collect()
    }

    pub fn take_host_requests(&mut self) -> Option<mpsc::Receiver<HostRequest>> {
        self.host_requests.take()
    }

    /// Transfers root-wide storage supervision to the binary process owner.
    pub fn take_storage_fatal_events(
        &mut self,
    ) -> Option<mpsc::Receiver<crate::storage::task_journal::TaskStorageFatalFailure>> {
        self.storage_fatal_events.take()
    }

    pub fn shared_gateway(&self) -> SharedRpcGateway {
        self.gateway.clone()
    }

    #[cfg(test)]
    pub(crate) fn attachment_runtime_for_test(
        &self,
    ) -> crate::attachment_runtime::AttachmentRuntime {
        self.attachment_runtime.clone()
    }

    pub fn handle_line(&mut self, line: &str) -> Vec<String> {
        let parsed = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(error) => return vec![serialize_message(parse_error(error))],
        };
        let values = match parsed {
            Value::Array(values) => values,
            value => vec![value],
        };
        values
            .into_iter()
            .flat_map(|value| self.handle_value(value))
            .map(serialize_message)
            .collect()
    }

    fn handle_value(&mut self, value: Value) -> Vec<WireMessage> {
        if let Some(response) = client_response(&value) {
            let now = self.next_time();
            return match self
                .gateway
                .handle_inbound(self.connection_id.clone(), response, now)
            {
                GatewayOutcome::Noop => Vec::new(),
                GatewayOutcome::Respond {
                    connection_id,
                    events,
                    server_requests,
                    ..
                } => event_wire_messages(connection_id.clone(), events)
                    .into_iter()
                    .chain(server_request_wire_messages(connection_id, server_requests))
                    .collect(),
            };
        }
        if self.host_bridge.try_handle_response(&value) {
            return Vec::new();
        }
        let request = match serde_json::from_value::<WireRequest>(value) {
            Ok(request) => request,
            Err(error) => return vec![invalid_request(None, error.to_string())],
        };
        let id = match request.id {
            WireRequestId::Notification => return Vec::new(),
            WireRequestId::Invalid => {
                return vec![invalid_request(
                    Some(Value::Null),
                    "invalid JSON-RPC id".to_string(),
                )];
            }
            WireRequestId::Request(id) => id,
        };
        if request.jsonrpc != "2.0" {
            return vec![invalid_request(Some(id), "jsonrpc must be 2.0".to_string())];
        }
        let Some(method) = request.method else {
            return vec![invalid_request(Some(id), "method is required".to_string())];
        };
        let params = request.params.unwrap_or_else(|| json!({}));
        let inbound = InboundProtocolMessage::ClientRequest {
            id: id_to_gateway_id(&id),
            method,
            params,
            meta: request.meta,
        };
        let now = self.next_time();
        match self
            .gateway
            .handle_inbound(self.connection_id.clone(), inbound, now)
        {
            GatewayOutcome::Respond {
                response,
                events,
                server_requests,
                ..
            } => wire_messages(
                id,
                self.connection_id.clone(),
                response,
                events,
                server_requests,
            ),
            GatewayOutcome::Noop => Vec::new(),
        }
    }

    pub fn handle_task_update(&mut self, update: TaskUpdate) -> Vec<String> {
        let now = self.next_time();
        let (events, server_requests) = self.gateway.publish_committed_task_update_for_connection(
            &self.connection_id,
            &update,
            now,
        );
        event_wire_messages(self.connection_id.clone(), events)
            .into_iter()
            .chain(server_request_wire_messages(
                self.connection_id.clone(),
                server_requests,
            ))
            .map(serialize_message)
            .collect()
    }

    fn next_time(&mut self) -> AppServerTime {
        let now = self.next_tick;
        self.next_tick += 1;
        AppServerTime(now)
    }
}

fn configured_project_roots_from_env() -> ConfiguredProjectRoots {
    let Some(paths) = env::var_os("OPENAIDE_PROJECT_ROOTS") else {
        return ConfiguredProjectRoots::default();
    };
    ConfiguredProjectRoots::from_workspace_roots(
        env::split_paths(&paths).map(|path| path.to_string_lossy().to_string()),
    )
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolEdgeStdioStartError {
    #[error("{0}")]
    Store(#[from] crate::storage::StoreOpenError),
    #[error("{0}")]
    AgentRegistry(#[from] crate::protocol::errors::RuntimeError),
}
