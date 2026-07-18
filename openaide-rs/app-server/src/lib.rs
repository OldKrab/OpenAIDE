pub mod agent;
pub(crate) mod agent_identity;
pub mod app_lifecycle;
pub mod app_server_client;
pub mod app_server_process;
pub(crate) mod attachment_runtime;
pub(crate) mod chat_history;
pub mod client_lifecycle;
pub mod diagnostics;
pub mod logging;
pub(crate) mod media;
pub mod projects;
pub mod protocol;
pub mod protocol_edge;
pub mod server_requests;
pub mod settings;
pub mod shell_file_handles;
pub mod snapshots;
pub mod state_sync;
pub mod storage;
pub mod storage_runtime;
pub mod task_events;
pub(crate) mod task_recovery;
pub mod tasks;
pub mod time;
pub mod transport;
pub(crate) mod workspace_file_index;
pub mod worktree_events;
pub mod worktrees;

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use agent::acp::AcpAgentRuntime;
use agent::acp_trace::{AcpTraceState, RuntimeSettings};
use agent::catalog_store::AgentCatalogStore;
use agent::registry::AgentRegistry;
use agent::registry_handle::AgentRegistryHandle;
use agent::AgentRuntime;
use diagnostics::RuntimeDiagnostics;
use protocol::host::{HostBridge, HostRequest};
use protocol::methods;
use storage::Store;
use task_events::{TaskUpdateNotifier, TaskUpdateReceiver};
use tasks::TaskService;

pub type RuntimeResult<T> = Result<T, protocol::errors::RuntimeError>;

pub struct Runtime {
    service: TaskService,
    host_bridge: HostBridge,
    acp_trace_state: AcpTraceState,
}

impl Runtime {
    pub fn new(storage_root: PathBuf) -> RuntimeResult<Self> {
        let store = Store::open(storage_root.clone())?;
        let acp_trace_state = AcpTraceState::from_env(&storage_root);
        let agent_registry =
            AgentRegistryHandle::new(AgentCatalogStore::new(store.clone()).registry()?);
        let agent_runtime =
            AcpAgentRuntime::new_with_registry(agent_registry.clone(), HostBridge::disabled());
        Self::new_with_open_store_and_agent(
            store,
            Arc::new(agent_runtime.with_trace_state(acp_trace_state.clone())),
            TaskUpdateNotifier::disabled(),
            HostBridge::disabled(),
            acp_trace_state,
            agent_registry,
        )
    }

    pub fn new_with_events(
        storage_root: PathBuf,
    ) -> RuntimeResult<(Self, TaskUpdateReceiver, mpsc::Receiver<HostRequest>)> {
        let store = Store::open(storage_root.clone())?;
        let (task_notifier, task_updates) = TaskUpdateNotifier::channel();
        let (host_bridge, host_requests) = HostBridge::channel();
        let acp_trace_state = AcpTraceState::from_env(&storage_root);
        let agent_registry =
            AgentRegistryHandle::new(AgentCatalogStore::new(store.clone()).registry()?);
        let agent_runtime =
            AcpAgentRuntime::new_with_registry(agent_registry.clone(), host_bridge.clone());
        let runtime = Self::new_with_open_store_and_agent(
            store,
            Arc::new(agent_runtime.with_trace_state(acp_trace_state.clone())),
            task_notifier,
            host_bridge,
            acp_trace_state,
            agent_registry,
        )?;
        Ok((runtime, task_updates, host_requests))
    }

    pub fn new_with_agent(
        storage_root: PathBuf,
        agent: Arc<dyn AgentRuntime>,
    ) -> RuntimeResult<Self> {
        Self::new_with_agent_and_task_update_notifier(
            storage_root,
            agent,
            TaskUpdateNotifier::disabled(),
            HostBridge::disabled(),
        )
    }

    pub fn new_with_agent_and_task_update_notifier(
        storage_root: PathBuf,
        agent: Arc<dyn AgentRuntime>,
        task_update_notifier: TaskUpdateNotifier,
        host_bridge: HostBridge,
    ) -> RuntimeResult<Self> {
        let acp_trace_state = AcpTraceState::disabled(&storage_root);
        Self::new_with_agent_and_task_update_notifier_and_trace(
            storage_root,
            agent,
            task_update_notifier,
            host_bridge,
            acp_trace_state,
            AgentRegistry::default_built_ins(),
        )
    }

    fn new_with_agent_and_task_update_notifier_and_trace(
        storage_root: PathBuf,
        agent: Arc<dyn AgentRuntime>,
        task_update_notifier: TaskUpdateNotifier,
        host_bridge: HostBridge,
        acp_trace_state: AcpTraceState,
        agent_registry: AgentRegistry,
    ) -> RuntimeResult<Self> {
        let store = Store::open(storage_root)?;
        Self::new_with_open_store_and_agent(
            store,
            agent,
            task_update_notifier,
            host_bridge,
            acp_trace_state,
            AgentRegistryHandle::new(agent_registry),
        )
    }

    fn new_with_open_store_and_agent(
        store: Store,
        agent: Arc<dyn AgentRuntime>,
        task_update_notifier: TaskUpdateNotifier,
        host_bridge: HostBridge,
        acp_trace_state: AcpTraceState,
        agent_registry: AgentRegistryHandle,
    ) -> RuntimeResult<Self> {
        Ok(Self {
            service: TaskService::open_with_task_update_notifier_and_agent_registry(
                store,
                agent,
                task_update_notifier,
                agent_registry.current(),
            )?,
            host_bridge,
            acp_trace_state,
        })
    }

    pub fn service(&self) -> &TaskService {
        &self.service
    }

    pub fn host_bridge(&self) -> HostBridge {
        self.host_bridge.clone()
    }

    pub fn diagnostics(&self) -> RuntimeResult<RuntimeDiagnostics> {
        Ok(RuntimeDiagnostics {
            status: "ready",
            version: env!("CARGO_PKG_VERSION").to_string(),
            method_count: methods::shell_local_methods().len(),
            tasks: self.service.diagnostics()?,
            redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed",
        })
    }

    pub fn settings(&self) -> RuntimeSettings {
        RuntimeSettings {
            developer: agent::acp_trace::RuntimeDeveloperSettings {
                acp_trace: self.acp_trace_state.status(),
            },
        }
    }

    pub fn update_settings(
        &self,
        params: protocol::params::RuntimeUpdateSettingsParams,
    ) -> RuntimeResult<RuntimeSettings> {
        if let Some(enabled) = params.developer.acp_trace.enabled {
            self.acp_trace_state.set_enabled(enabled)?;
        }
        Ok(self.settings())
    }
}
