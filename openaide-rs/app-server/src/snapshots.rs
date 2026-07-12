use openaide_app_server_protocol::client::RequestedSurface;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use std::sync::Arc;

use openaide_app_server_protocol::ids::{AgentId, ProjectId, ServerId, StateRootId, TaskId};
use openaide_app_server_protocol::snapshot::{
    AgentCollectionSnapshot, ChatSnapshot, ClientSnapshot, ClientSnapshotScope,
    LiveSessionDataState, ProjectCollectionSnapshot, ProtocolVersion, ServerCapabilities,
    ServerSnapshot, StateRootSnapshot, TaskAgentCommandsSnapshot, TaskAgentConfigSnapshot,
    TaskLifecycle, TaskNavigationSnapshot, TaskPreparationAction, TaskPreparationSnapshot,
    TaskSendBlocker, TaskSendBlockerKind, TaskSendCapabilitySnapshot, TaskSendCapabilityState,
    TaskSetupBlocker, TaskSetupBlockerKind, TaskSnapshot, TaskStatus, TaskSummary,
};
use openaide_app_server_protocol::state::{SubscriptionScope, SubscriptionSnapshot};

use crate::client_lifecycle::ClientContext;
use crate::settings::{SettingsCatalog, SettingsSnapshotSource};
use crate::storage_runtime::SnapshotReadToken;

mod agent_collection;
mod project_collection;
mod task_navigation;
pub(crate) mod task_snapshot;

pub use agent_collection::{AgentCollectionSnapshotSource, AgentRegistrySnapshotSource};
pub use project_collection::{ProjectCollectionSnapshotSource, ProjectCollectionStore};
pub(crate) use task_navigation::project_task_summary;
pub use task_navigation::{TaskNavigationSnapshotSource, TaskNavigationStore};
pub use task_snapshot::{TaskListSnapshot, TaskSnapshotSource, TaskSnapshotStore};

pub trait SnapshotProvider {
    fn snapshot(
        &self,
        ctx: &ClientContext,
        scope: &SubscriptionScope,
        token: &SnapshotReadToken,
    ) -> Result<SubscriptionSnapshot, ProtocolError>;
}

#[derive(Clone)]
pub struct SnapshotBuilder {
    server_id: ServerId,
    state_root_id: StateRootId,
    agents: Arc<dyn AgentCollectionSnapshotSource>,
    projects: Arc<dyn ProjectCollectionSnapshotSource>,
    settings: Arc<dyn SettingsSnapshotSource>,
    task_navigation: Arc<dyn TaskNavigationSnapshotSource>,
    task_snapshots: Arc<dyn TaskSnapshotSource>,
}

impl SnapshotBuilder {
    pub fn new(server_id: ServerId, state_root_id: StateRootId) -> Self {
        Self::with_task_navigation(server_id, state_root_id, Arc::new(EmptyTaskNavigation))
    }

    pub fn with_task_navigation(
        server_id: ServerId,
        state_root_id: StateRootId,
        task_navigation: Arc<dyn TaskNavigationSnapshotSource>,
    ) -> Self {
        Self::with_sources(
            server_id,
            state_root_id,
            Arc::new(EmptyAgentCollection),
            Arc::new(EmptyProjectCollection),
            Arc::new(SettingsCatalog::default()),
            task_navigation,
            Arc::new(EmptyTaskSnapshots),
        )
    }

    pub fn with_sources(
        server_id: ServerId,
        state_root_id: StateRootId,
        agents: Arc<dyn AgentCollectionSnapshotSource>,
        projects: Arc<dyn ProjectCollectionSnapshotSource>,
        settings: Arc<dyn SettingsSnapshotSource>,
        task_navigation: Arc<dyn TaskNavigationSnapshotSource>,
        task_snapshots: Arc<dyn TaskSnapshotSource>,
    ) -> Self {
        Self {
            server_id,
            state_root_id,
            agents,
            projects,
            settings,
            task_navigation,
            task_snapshots,
        }
    }

    pub fn client_snapshot(
        &self,
        ctx: &ClientContext,
        requested_surface: RequestedSurface,
        token: &SnapshotReadToken,
    ) -> Result<ClientSnapshot, ProtocolError> {
        let active_task = match &requested_surface {
            RequestedSurface::Task { task_id } => Some(
                self.task_snapshots
                    .open_for_client(&ctx.client_instance_id, task_id)?,
            ),
            _ => None,
        };
        Ok(ClientSnapshot {
            cursor: token.cursor().clone(),
            server: ServerSnapshot {
                server_id: self.server_id.clone(),
                protocol_version: ProtocolVersion::V1,
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
            projects: Some(self.projects.snapshot()?),
            agents: Some(self.agents.snapshot()?),
            tasks: Some(self.task_navigation.snapshot(None)?),
            active_task,
            settings: Some(self.settings.snapshot(None)?),
            pending_requests: Vec::new(),
        })
    }

    pub(crate) fn project_collection_snapshot(
        &self,
    ) -> Result<ProjectCollectionSnapshot, ProtocolError> {
        self.projects.snapshot()
    }
}

impl SnapshotProvider for SnapshotBuilder {
    fn snapshot(
        &self,
        ctx: &ClientContext,
        scope: &SubscriptionScope,
        _token: &SnapshotReadToken,
    ) -> Result<SubscriptionSnapshot, ProtocolError> {
        Ok(match scope {
            SubscriptionScope::Projects => SubscriptionSnapshot::Projects {
                projects: self.projects.snapshot()?,
            },
            SubscriptionScope::Agents => SubscriptionSnapshot::Agents {
                agents: self.agents.snapshot()?,
            },
            SubscriptionScope::Settings { .. } => SubscriptionSnapshot::Settings {
                settings: self.settings.snapshot(match scope {
                    SubscriptionScope::Settings { section } => *section,
                    _ => None,
                })?,
            },
            SubscriptionScope::TaskNavigation { project_id } => {
                SubscriptionSnapshot::TaskNavigation {
                    navigation: self.task_navigation.snapshot(project_id.as_ref())?,
                }
            }
            SubscriptionScope::Task { task_id } => SubscriptionSnapshot::Task {
                task: self
                    .task_snapshots
                    .open_for_client(&ctx.client_instance_id, task_id)?,
            },
        })
    }
}

#[derive(Debug)]
struct EmptyAgentCollection;

impl AgentCollectionSnapshotSource for EmptyAgentCollection {
    fn snapshot(&self) -> Result<AgentCollectionSnapshot, ProtocolError> {
        Ok(AgentCollectionSnapshot {
            agents: Vec::new(),
            default_agent_id: None,
        })
    }
}

#[derive(Debug)]
struct EmptyProjectCollection;

impl ProjectCollectionSnapshotSource for EmptyProjectCollection {
    fn snapshot(&self) -> Result<ProjectCollectionSnapshot, ProtocolError> {
        Ok(ProjectCollectionSnapshot {
            projects: Vec::new(),
            active_project_id: None,
        })
    }
}

#[derive(Debug)]
struct EmptyTaskNavigation;

impl TaskNavigationSnapshotSource for EmptyTaskNavigation {
    fn snapshot(
        &self,
        _project_id: Option<&ProjectId>,
    ) -> Result<TaskNavigationSnapshot, ProtocolError> {
        Ok(TaskNavigationSnapshot {
            tasks: Vec::new(),
            active_task_id: None,
        })
    }
}

#[derive(Debug)]
struct EmptyTaskSnapshots;

impl TaskSnapshotSource for EmptyTaskSnapshots {
    fn list(
        &self,
        _archived: bool,
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
}

fn unavailable_task_snapshot(task_id: TaskId) -> TaskSnapshot {
    TaskSnapshot {
        task: TaskSummary {
            task_id,
            project_id: ProjectId::from("unavailable-project"),
            agent_id: AgentId::from("unavailable-agent"),
            title: "Task unavailable".to_string(),
            status: TaskStatus::Failed,
            updated_at: "1970-01-01T00:00:00.000Z".to_string(),
            last_activity: "1970-01-01T00:00:00.000Z".to_string(),
            unread: false,
            has_messages: false,
        },
        lifecycle: TaskLifecycle::Visible,
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
            attachment_only: false,
            blockers: vec![TaskSendBlocker {
                kind: TaskSendBlockerKind::TaskPreparing,
                message: "Task workflow is not available".to_string(),
            }],
        },
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
