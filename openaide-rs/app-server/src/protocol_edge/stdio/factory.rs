use std::sync::Arc;
use uuid::Uuid;

use crate::agent::{
    catalog_store::AgentCatalogStore, product_api::AgentProductApi,
    registry_handle::AgentRegistryHandle, status_cache::AgentStatusCache, AgentRuntime,
};
use crate::app_lifecycle::AppLifecycle;
use crate::client_lifecycle::{ClientHub, ClientLivenessPolicy};
use crate::diagnostics::RuntimeDiagnosticsService;
use crate::projects::{ConfiguredProjectRoots, StorageProjectResolver};
use crate::protocol_edge::{AppServerProbeFacts, RpcGateway};
use crate::server_requests::ServerRequestRuntime;
use crate::settings::{
    AppPreferencesService, McpServersSettingsService, RuntimeSettingsService, SettingsCatalog,
    SkillsSettingsService,
};
use crate::shell_file_handles::ShellFileRevealRegistry;
use crate::snapshots::{
    AgentRegistrySnapshotSource, ProjectCollectionStore, SnapshotBuilder, SnapshotSources,
    TaskNavigationStore, TaskSnapshotStore,
};
use crate::state_sync::StateStream;
use crate::storage::Store;
use crate::storage_runtime::StateRoot;
use crate::task_events::{TaskUpdateNotifier, TaskUpdateReceiver};
use crate::tasks::product_api::TaskProductApi;
use crate::worktree_events::{WorktreeUpdateNotifier, WorktreeUpdateReceiver};

use super::ProtocolEdgeStdioStartError;

pub(super) struct GatewayFactoryOutput {
    pub gateway: RpcGateway,
    pub task_updates: TaskUpdateReceiver,
    pub worktree_updates: WorktreeUpdateReceiver,
    pub storage_fatal_events:
        std::sync::mpsc::Receiver<crate::storage::task_journal::TaskStorageFatalFailure>,
    #[cfg(test)]
    pub attachment_runtime: crate::attachment_runtime::AttachmentRuntime,
}

pub(super) fn gateway(
    state_root: StateRoot,
    store: Store,
    agent_registry: AgentRegistryHandle,
    agent_runtime: Arc<dyn AgentRuntime>,
    acp_trace_state: crate::agent::acp_trace::AcpTraceState,
    configured_projects: ConfiguredProjectRoots,
) -> Result<GatewayFactoryOutput, ProtocolEdgeStdioStartError> {
    let storage_fatal_events = store.take_task_storage_fatal_events();
    let (task_notifier, task_updates) = TaskUpdateNotifier::channel();
    let projects = ProjectCollectionStore::new_with_configured_roots(
        store.clone(),
        configured_projects.clone(),
    );
    let project_resolver = StorageProjectResolver::new_with_configured_roots(
        store.clone(),
        configured_projects.clone(),
    );
    let server_requests = ServerRequestRuntime::new();
    let shell_file_reveals = ShellFileRevealRegistry::new();
    let app_preferences = Arc::new(AppPreferencesService::new(store.clone()));
    let runtime_settings = Arc::new(RuntimeSettingsService::new(acp_trace_state.clone()));
    let mcp_servers_settings = Arc::new(McpServersSettingsService::new());
    let skills_settings = Arc::new(SkillsSettingsService::new());
    let agent_statuses = AgentStatusCache::default();
    let agent_snapshots = AgentRegistrySnapshotSource::with_status_cache(
        agent_registry.clone(),
        agent_statuses.clone(),
    );
    let agent_product_api = AgentProductApi::new(
        agent_registry.clone(),
        AgentCatalogStore::new(store.clone()),
        agent_runtime.clone(),
        agent_statuses.clone(),
    );
    let task_navigation_agents = agent_registry.clone();
    let task_product_api = Arc::new(TaskProductApi::new_with_server_requests_and_projects(
        store.clone(),
        Arc::new(project_resolver),
        agent_registry,
        agent_runtime,
        task_notifier,
        server_requests.clone(),
        configured_projects.clone(),
    )?);
    let task_navigation = TaskNavigationStore::with_native_sessions_and_agents(
        store.clone(),
        task_product_api.native_session_catalog(),
        task_navigation_agents,
    );
    let task_snapshots = Arc::new(TaskSnapshotStore::with_history_sync(
        store.clone(),
        task_product_api.history_sync_snapshots(),
    ));
    let (worktree_notifier, worktree_updates) = WorktreeUpdateNotifier::channel();
    let worktrees = Arc::new(
        crate::worktrees::WorktreeManager::with_notifier_and_cleanup(
            store.clone(),
            worktree_notifier,
            task_product_api.clone(),
            configured_projects.clone(),
        ),
    );
    #[cfg(test)]
    let attachment_runtime = task_product_api.attachment_runtime();
    let state_root_id = state_root.fingerprint().as_str().to_string();
    // A gateway construction is one App Server process epoch. Replicas use this ID to
    // distinguish restarts, while SnapshotBuilder keeps it stable for every snapshot in the epoch.
    let server_id = Uuid::new_v4().to_string();
    let gateway = RpcGateway::new(
        ClientHub::new(ClientLivenessPolicy::new(10_000, 30_000)),
        AppLifecycle::new(),
        StateStream::new(state_root_id.clone().into()),
        server_requests,
        shell_file_reveals,
        SnapshotBuilder::with_sources(
            server_id.into(),
            state_root_id.into(),
            SnapshotSources::new(
                Arc::new(store.clone()),
                Arc::new(agent_snapshots),
                Arc::new(projects),
                worktrees.clone(),
                Arc::new(SettingsCatalog::with_backend_settings(
                    app_preferences.clone(),
                    runtime_settings.clone(),
                )),
                Arc::new(task_navigation),
                task_snapshots.clone(),
            ),
        ),
        task_snapshots,
        configured_projects,
        AppServerProbeFacts::new(state_root.fingerprint().as_str()),
        Arc::new(RuntimeDiagnosticsService::new(store.clone())),
        Arc::new(agent_product_api.clone()),
        Arc::new(agent_product_api.clone()),
        Arc::new(agent_product_api.clone()),
        Arc::new(agent_product_api),
        mcp_servers_settings,
        skills_settings,
        app_preferences,
        runtime_settings,
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        task_product_api.clone(),
        worktrees,
        task_product_api,
    );
    Ok(GatewayFactoryOutput {
        gateway,
        task_updates,
        worktree_updates,
        storage_fatal_events,
        #[cfg(test)]
        attachment_runtime,
    })
}
