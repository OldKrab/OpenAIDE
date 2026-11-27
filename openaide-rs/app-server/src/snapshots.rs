use openaide_app_server_protocol::client::RequestedSurface;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use std::sync::Arc;

use openaide_app_server_protocol::global_state::GlobalState;
use openaide_app_server_protocol::ids::{AgentId, ProjectId, ServerId, StateRootId, TaskId};
use openaide_app_server_protocol::snapshot::{
    AgentCollectionSnapshot, ChatSnapshot, ClientSnapshot, ClientSnapshotScope,
    LiveSessionDataState, NewTaskDefaultsSnapshot, ProjectCollectionSnapshot, ServerCapabilities,
    ServerSnapshot, SettingsSnapshot, StateRootSnapshot, TaskAgentCommandsSnapshot,
    TaskAgentConfigSnapshot, TaskLifecycle, TaskPreparationAction, TaskPreparationSnapshot,
    TaskSendBlocker, TaskSendBlockerKind, TaskSendCapabilitySnapshot, TaskSendCapabilityState,
    TaskSetupBlocker, TaskSetupBlockerKind, TaskSnapshot, TaskStatus, TaskSummary,
};
use openaide_app_server_protocol::state::{SubscriptionScope, SubscriptionSnapshot};

use crate::client_lifecycle::ClientContext;
use crate::settings::{SettingsCatalog, SettingsSnapshotSource};

mod agent_collection;
mod project_collection;
mod task_navigation;
pub(crate) mod task_snapshot;
mod worktree_repository;

pub use agent_collection::{AgentCollectionSnapshotSource, AgentRegistrySnapshotSource};
pub use project_collection::{ProjectCollectionSnapshotSource, ProjectCollectionStore};
pub use task_navigation::TaskNavigationStore;
pub(crate) use task_navigation::{
    native_session_summary, project_task_lifecycle, project_task_summary,
};
pub use task_snapshot::{TaskListSnapshot, TaskSnapshotSource, TaskSnapshotStore};
pub use worktree_repository::WorktreeRepositorySnapshotSource;

pub trait SnapshotProvider {
    /// Builds the baseline for a subscription. `known_version` is the content version the client
    /// holds; only a Task, Subagent History or Tool detail scope honors it.
    fn snapshot(
        &self,
        ctx: &ClientContext,
        scope: &SubscriptionScope,
        known_version: Option<u64>,
    ) -> Result<SubscriptionSnapshot, ProtocolError>;
}

/// A Task subscription's reply (ADR-0086): every current field, and the Chat as `chat_sync`
/// says. See `SubscriptionSnapshot::Task`.
#[derive(Debug, Clone)]
pub struct TaskSubscription {
    pub task: openaide_app_server_protocol::snapshot::TaskSnapshot,
    pub chat_version: u64,
    pub chat_sync: openaide_app_server_protocol::state::TaskChatSync,
}

/// A Subagent History subscription's reply: see `SubscriptionSnapshot::SubagentHistory`.
#[derive(Debug, Clone)]
pub struct SubagentHistorySubscription {
    pub history: openaide_app_server_protocol::snapshot::SubagentHistorySnapshot,
    pub chat_sync: openaide_app_server_protocol::state::SubagentChatSync,
}

/// A Tool detail subscription's reply: see `SubscriptionSnapshot::ToolDetail`.
#[derive(Debug, Clone)]
pub struct ToolDetailSubscription {
    pub details: openaide_app_server_protocol::task::ToolDetailSnapshot,
    pub terminal_sync: openaide_app_server_protocol::state::ToolTerminalSync,
}

pub trait NewTaskDefaultsSnapshotSource: Send + Sync {
    fn snapshot(&self) -> Result<NewTaskDefaultsSnapshot, ProtocolError>;
}

impl NewTaskDefaultsSnapshotSource for crate::storage::Store {
    fn snapshot(&self) -> Result<NewTaskDefaultsSnapshot, ProtocolError> {
        self.read_new_task_defaults()
            .map_err(|error| ProtocolError {
                code: ProtocolErrorCode::Internal,
                message: format!("Failed to read New Task defaults: {error}"),
                recoverable: true,
                target: None,
            })
    }
}

#[derive(Clone)]
pub struct SnapshotBuilder {
    server_id: ServerId,
    state_root_id: StateRootId,
    new_task_defaults: Arc<dyn NewTaskDefaultsSnapshotSource>,
    agents: Arc<dyn AgentCollectionSnapshotSource>,
    projects: Arc<dyn ProjectCollectionSnapshotSource>,
    worktrees: Arc<dyn WorktreeRepositorySnapshotSource>,
    settings: Arc<dyn SettingsSnapshotSource>,
    task_snapshots: Arc<dyn TaskSnapshotSource>,
    devices: Arc<dyn crate::remote_devices::RemoteDevicesWorkflow>,
    terminals: crate::terminals::Terminals,
}

/// Groups snapshot projections so adding one source does not widen every construction call.
pub struct SnapshotSources {
    new_task_defaults: Arc<dyn NewTaskDefaultsSnapshotSource>,
    agents: Arc<dyn AgentCollectionSnapshotSource>,
    projects: Arc<dyn ProjectCollectionSnapshotSource>,
    worktrees: Arc<dyn WorktreeRepositorySnapshotSource>,
    settings: Arc<dyn SettingsSnapshotSource>,
    task_snapshots: Arc<dyn TaskSnapshotSource>,
}

impl SnapshotSources {
    pub fn new(
        new_task_defaults: Arc<dyn NewTaskDefaultsSnapshotSource>,
        agents: Arc<dyn AgentCollectionSnapshotSource>,
        projects: Arc<dyn ProjectCollectionSnapshotSource>,
        worktrees: Arc<dyn WorktreeRepositorySnapshotSource>,
        settings: Arc<dyn SettingsSnapshotSource>,
        task_snapshots: Arc<dyn TaskSnapshotSource>,
    ) -> Self {
        Self {
            new_task_defaults,
            agents,
            projects,
            worktrees,
            settings,
            task_snapshots,
        }
    }
}

impl SnapshotBuilder {
    pub fn new(server_id: ServerId, state_root_id: StateRootId) -> Self {
        Self::with_sources(
            server_id,
            state_root_id,
            SnapshotSources::new(
                Arc::new(EmptyNewTaskDefaults),
                Arc::new(EmptyAgentCollection),
                Arc::new(EmptyProjectCollection),
                Arc::new(EmptyWorktreeRepositories),
                Arc::new(SettingsCatalog::default()),
                Arc::new(EmptyTaskSnapshots),
            ),
        )
    }

    #[cfg(test)]
    pub(crate) fn with_task_snapshots(
        server_id: ServerId,
        state_root_id: StateRootId,
        task_snapshots: Arc<dyn TaskSnapshotSource>,
    ) -> Self {
        Self::with_sources(
            server_id,
            state_root_id,
            SnapshotSources::new(
                Arc::new(EmptyNewTaskDefaults),
                Arc::new(EmptyAgentCollection),
                Arc::new(EmptyProjectCollection),
                Arc::new(EmptyWorktreeRepositories),
                Arc::new(SettingsCatalog::default()),
                task_snapshots,
            ),
        )
    }

    pub fn with_sources(
        server_id: ServerId,
        state_root_id: StateRootId,
        sources: SnapshotSources,
    ) -> Self {
        Self {
            server_id,
            state_root_id,
            new_task_defaults: sources.new_task_defaults,
            agents: sources.agents,
            projects: sources.projects,
            worktrees: sources.worktrees,
            settings: sources.settings,
            task_snapshots: sources.task_snapshots,
            devices: Arc::new(crate::remote_devices::NoRemoteDevices),
            terminals: crate::terminals::Terminals::default(),
        }
    }

    pub(crate) fn with_remote_devices(
        mut self,
        devices: Arc<dyn crate::remote_devices::RemoteDevicesWorkflow>,
    ) -> Self {
        self.devices = devices;
        self
    }

    /// The gateway and subscriptions read one registry, so a response and a snapshot agree.
    pub(crate) fn with_terminals(mut self, terminals: crate::terminals::Terminals) -> Self {
        self.terminals = terminals;
        self
    }

    /// The initialize reply. `global` is what the client's global-state owner holds for it.
    pub fn client_snapshot(
        &self,
        ctx: &ClientContext,
        requested_surface: RequestedSurface,
        global: GlobalState,
    ) -> Result<ClientSnapshot, ProtocolError> {
        let active_task = match &requested_surface {
            RequestedSurface::Task { task_id } => {
                match self
                    .task_snapshots
                    .open_for_client(&ctx.client_instance_id, task_id)
                {
                    Ok(task) => Some(task),
                    // Client initialization owns the global baseline. The routed
                    // task/open request reports a missing Task without poisoning it.
                    Err(error) if error.code == ProtocolErrorCode::NotFound => None,
                    Err(error) => return Err(error),
                }
            }
            _ => None,
        };
        Ok(ClientSnapshot {
            server: ServerSnapshot {
                server_id: self.server_id.clone(),
                protocol_version: openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION,
                capabilities: ServerCapabilities {
                    reconnect: true,
                    resync: true,
                    streaming_events: true,
                    frontend_requests: true,
                },
            },
            state_root: StateRootSnapshot {
                state_root_id: self.state_root_id.clone(),
            },
            client: ClientSnapshotScope {
                client_instance_id: ctx.client_instance_id.clone(),
                shell_kind: ctx.shell.kind,
                surface: requested_surface,
            },
            global,
            active_task,
        })
    }

    /// Reads every global-state area from its source: the owner's first baseline.
    pub(crate) fn read_global_state(&self) -> Result<GlobalState, ProtocolError> {
        let projects = self.projects.snapshot()?;
        let worktrees = projects
            .projects
            .iter()
            .filter_map(|project| project.worktree_repository_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter_map(|repository_id| self.worktree_repository(&repository_id).ok())
            .collect();
        Ok(GlobalState {
            projects,
            worktrees,
            agents: self.agents.snapshot()?,
            settings: self.settings.snapshot(None)?,
            new_task_defaults: self.new_task_defaults.snapshot()?,
            devices: self.devices.snapshot(),
            terminals: self.terminals.snapshot(),
        })
    }

    pub(crate) fn settings_snapshot(&self) -> Result<SettingsSnapshot, ProtocolError> {
        self.settings.snapshot(None)
    }

    pub(crate) fn new_task_defaults_snapshot(
        &self,
    ) -> Result<NewTaskDefaultsSnapshot, ProtocolError> {
        self.new_task_defaults.snapshot()
    }

    pub(crate) fn device_collection_snapshot(
        &self,
    ) -> openaide_app_server_protocol::devices::DeviceCollectionSnapshot {
        self.devices.snapshot()
    }

    pub(crate) fn terminal_collection_snapshot(
        &self,
    ) -> openaide_app_server_protocol::terminals::TerminalCollectionSnapshot {
        self.terminals.snapshot()
    }

    pub(crate) fn worktree_repository(
        &self,
        repository_id: &openaide_app_server_protocol::ids::WorktreeRepositoryId,
    ) -> Result<openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot, ProtocolError>
    {
        self.worktrees.snapshot(repository_id)
    }

    pub(crate) fn project_collection_snapshot(
        &self,
    ) -> Result<ProjectCollectionSnapshot, ProtocolError> {
        self.projects.snapshot()
    }

    pub(crate) fn agent_collection_snapshot(
        &self,
    ) -> Result<AgentCollectionSnapshot, ProtocolError> {
        self.agents.snapshot()
    }
}

impl SnapshotProvider for SnapshotBuilder {
    fn snapshot(
        &self,
        ctx: &ClientContext,
        scope: &SubscriptionScope,
        known_version: Option<u64>,
    ) -> Result<SubscriptionSnapshot, ProtocolError> {
        Ok(match scope {
            // Navigation is a view subscription (`navigation/subscribe`), not an entity snapshot.
            SubscriptionScope::TaskNavigation { .. } => {
                return Err(ProtocolError {
                    code: openaide_app_server_protocol::errors::ProtocolErrorCode::InvalidRequest,
                    message: "Task navigation is subscribed with navigation/subscribe".to_string(),
                    recoverable: false,
                    target: None,
                })
            }
            SubscriptionScope::Task { task_id } => {
                let subscription = self.task_snapshots.subscription_for_client(
                    &ctx.client_instance_id,
                    task_id,
                    known_version,
                )?;
                SubscriptionSnapshot::Task {
                    task: subscription.task,
                    chat_version: subscription.chat_version,
                    chat_sync: subscription.chat_sync,
                }
            }
            SubscriptionScope::SubagentCatalog { task_id } => {
                SubscriptionSnapshot::SubagentCatalog {
                    catalog: self
                        .task_snapshots
                        .subagent_catalog_for_client(&ctx.client_instance_id, task_id)?,
                }
            }
            SubscriptionScope::SubagentHistory {
                task_id,
                subagent_id,
            } => {
                let subscription = self.task_snapshots.subagent_history_for_client(
                    &ctx.client_instance_id,
                    task_id,
                    subagent_id,
                    known_version,
                )?;
                SubscriptionSnapshot::SubagentHistory {
                    history: subscription.history,
                    chat_sync: subscription.chat_sync,
                }
            }
            SubscriptionScope::ToolDetail {
                task_id,
                artifact_id,
            } => {
                let subscription = self.task_snapshots.tool_detail_for_client(
                    &ctx.client_instance_id,
                    task_id,
                    artifact_id,
                    known_version,
                )?;
                SubscriptionSnapshot::ToolDetail {
                    task_id: task_id.clone(),
                    artifact_id: artifact_id.clone(),
                    details: subscription.details,
                    terminal_sync: subscription.terminal_sync,
                }
            }
        })
    }
}

#[derive(Debug)]
struct EmptyAgentCollection;

#[derive(Debug)]
struct EmptyNewTaskDefaults;

impl NewTaskDefaultsSnapshotSource for EmptyNewTaskDefaults {
    fn snapshot(&self) -> Result<NewTaskDefaultsSnapshot, ProtocolError> {
        Ok(NewTaskDefaultsSnapshot::default())
    }
}

impl AgentCollectionSnapshotSource for EmptyAgentCollection {
    fn snapshot(&self) -> Result<AgentCollectionSnapshot, ProtocolError> {
        Ok(AgentCollectionSnapshot { agents: Vec::new() })
    }
}

#[derive(Debug)]
struct EmptyProjectCollection;

impl ProjectCollectionSnapshotSource for EmptyProjectCollection {
    fn snapshot(&self) -> Result<ProjectCollectionSnapshot, ProtocolError> {
        Ok(ProjectCollectionSnapshot {
            projects: Vec::new(),
        })
    }
}

#[derive(Debug)]
struct EmptyWorktreeRepositories;

impl WorktreeRepositorySnapshotSource for EmptyWorktreeRepositories {
    fn snapshot(
        &self,
        _repository_id: &openaide_app_server_protocol::ids::WorktreeRepositoryId,
    ) -> Result<openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot, ProtocolError>
    {
        Err(ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message: "Worktree Repository is unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

#[derive(Debug)]
struct EmptyTaskSnapshots;

impl TaskSnapshotSource for EmptyTaskSnapshots {
    fn list(
        &self,
        _lifecycle: openaide_app_server_protocol::task::TaskListLifecycle,
        _project_id: Option<&ProjectId>,
        _cursor: Option<&openaide_app_server_protocol::ids::TaskListCursor>,
    ) -> Result<TaskListSnapshot, ProtocolError> {
        Ok(TaskListSnapshot {
            tasks: Vec::new(),
            revision: 0,
            next_cursor: None,
        })
    }

    fn open_internal(&self, task_id: &TaskId) -> Result<TaskSnapshot, ProtocolError> {
        Ok(unavailable_task_snapshot(task_id.clone()))
    }

    fn open_for_client(
        &self,
        _client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<TaskSnapshot, ProtocolError> {
        Ok(unavailable_task_snapshot(task_id.clone()))
    }

    fn tool_detail_for_client(
        &self,
        _client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        _task_id: &TaskId,
        _artifact_id: &str,
        _known_version: Option<u64>,
    ) -> Result<ToolDetailSubscription, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message: "Tool detail is unavailable".to_string(),
            recoverable: false,
            target: None,
        })
    }

    fn tool_image_preview_for_client(
        &self,
        _client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        _task_id: &TaskId,
        _artifact_id: &str,
    ) -> Result<Option<openaide_app_server_protocol::task::ToolImagePreview>, ProtocolError> {
        Ok(None)
    }
}

fn unavailable_task_snapshot(task_id: TaskId) -> TaskSnapshot {
    TaskSnapshot {
        task: TaskSummary {
            task_id,
            project_id: ProjectId::from("unavailable-project"),
            agent_id: AgentId::from("unavailable-agent"),
            lifecycle: TaskLifecycle::Open,
            title: None,
            status: TaskStatus::Failed,
            updated_at: "1970-01-01T00:00:00.000Z".to_string(),
            last_activity: "1970-01-01T00:00:00.000Z".to_string(),
            unread: false,
            pinned: false,
            attention: None,
            has_messages: false,
            worktree_id: None,
            workspace_available: false,
        },
        permission_policy: Default::default(),
        active_turn_started_at: None,
        background_commands: Vec::new(),
        lifecycle: TaskLifecycle::Open,
        revision: 0,
        preparation: TaskPreparationSnapshot::Blocked {
            blocker: TaskSetupBlocker {
                kind: TaskSetupBlockerKind::CapabilityUnavailable,
                message: "Task workflow snapshots are not available in this App Server slice"
                    .to_string(),
            },
            actions: vec![TaskPreparationAction::Retry],
        },
        agent_config: TaskAgentConfigSnapshot {
            state: LiveSessionDataState::Unavailable,
            options: Vec::new(),
            pending_change: None,
            preferences: None,
            error: Some(ProtocolError {
                code: ProtocolErrorCode::CapabilityUnavailable,
                message: "Task workflow is not available".to_string(),
                recoverable: true,
                target: None,
            }),
        },
        agent_commands: TaskAgentCommandsSnapshot {
            state: LiveSessionDataState::Unavailable,
            commands: Vec::new(),
            error: None,
        },
        send_capability: TaskSendCapabilitySnapshot {
            state: TaskSendCapabilityState::Blocked,
            blockers: vec![TaskSendBlocker {
                kind: TaskSendBlockerKind::TaskPreparing,
                message: "Task workflow is not available".to_string(),
            }],
        },
        input_capabilities: None,
        context_usage: None,
        subagents: Default::default(),
        current_plan: None,
        message_queue: Default::default(),
        chat: ChatSnapshot {
            items: Vec::new(),
            has_more_before: false,
            has_messages: false,
            start_cursor: None,
            end_cursor: None,
        },
        pending_requests: Vec::new(),
        recovery: None,
        history_sync: Default::default(),
    }
}

#[cfg(test)]
mod tests;
