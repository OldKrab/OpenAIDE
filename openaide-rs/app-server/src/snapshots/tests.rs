use std::sync::Arc;

use openaide_app_server_protocol::client::{
    ClientCapabilities, RequestedSurface, SettingsSection, ShellDescriptor, ShellKind,
};
use openaide_app_server_protocol::ids::{AgentId, ClientInstanceId, TaskId};
use openaide_app_server_protocol::snapshot::{
    AgentStatus, ChatSnapshot, LiveSessionDataState, ProjectCollectionSnapshot, ProjectSummary,
    TaskAgentCommandsSnapshot, TaskAgentConfigSnapshot, TaskPreparationSnapshot,
    TaskSendCapabilitySnapshot, TaskSendCapabilityState, TaskSnapshot, TaskStatus, TaskSummary,
};
use openaide_app_server_protocol::state::{SubscriptionScope, SubscriptionSnapshot};

use super::{
    AgentRegistrySnapshotSource, EmptyTaskNavigation, ProjectCollectionSnapshotSource,
    SnapshotBuilder, SnapshotProvider, SnapshotSources, TaskListSnapshot, TaskSnapshotSource,
};
use crate::agent::registry::{AgentRegistry, CODEX_AGENT_ID};
use crate::client_lifecycle::{ClientContext, ConnectionId};
use crate::settings::SettingsCatalog;
use crate::storage_runtime::CursorSequencer;

#[test]
fn client_snapshot_includes_backend_owned_agent_collection() {
    let builder = builder();

    let snapshot = builder
        .client_snapshot(
            &ctx(),
            RequestedSurface::Home,
            &CursorSequencer::new().read_token(),
        )
        .unwrap();

    let agents = snapshot.agents.unwrap();
    assert_eq!(agents.agents.len(), 2);
    assert!(agents
        .agents
        .iter()
        .all(|agent| agent.status == AgentStatus::Disconnected));
}

#[test]
fn client_snapshot_includes_backend_owned_project_collection() {
    let snapshot = builder()
        .client_snapshot(
            &ctx(),
            RequestedSurface::Home,
            &CursorSequencer::new().read_token(),
        )
        .unwrap();

    let projects = snapshot.projects.unwrap();
    assert_eq!(projects.projects[0].project_id.as_str(), "project-1");
    assert_eq!(projects.projects[0].label, "Project");
}

#[test]
fn client_snapshot_keeps_new_task_defaults_separate_from_collections() {
    let snapshot = builder()
        .client_snapshot(
            &ctx(),
            RequestedSurface::Home,
            &CursorSequencer::new().read_token(),
        )
        .unwrap();

    assert_eq!(
        snapshot.new_task_defaults.project_id,
        Some("project-1".into())
    );
    assert_eq!(snapshot.new_task_defaults.agent_id, Some("codex".into()));
}

#[test]
fn agent_subscription_uses_backend_owned_agent_collection() {
    let SubscriptionSnapshot::Agents { agents } = builder()
        .snapshot(
            &ctx(),
            &SubscriptionScope::Agents,
            &CursorSequencer::new().read_token(),
        )
        .unwrap()
    else {
        panic!("expected agents snapshot");
    };

    assert_eq!(agents.agents[0].agent_id, AgentId::from(CODEX_AGENT_ID));
}

#[test]
fn project_subscription_uses_backend_owned_project_collection() {
    let SubscriptionSnapshot::Projects { projects } = builder()
        .snapshot(
            &ctx(),
            &SubscriptionScope::Projects,
            &CursorSequencer::new().read_token(),
        )
        .unwrap()
    else {
        panic!("expected projects snapshot");
    };

    assert_eq!(projects.projects[0].project_id.as_str(), "project-1");
    assert_eq!(projects.projects[0].label, "Project");
}

#[test]
fn client_snapshot_includes_backend_owned_settings_collection() {
    let snapshot = builder()
        .client_snapshot(
            &ctx(),
            RequestedSurface::Home,
            &CursorSequencer::new().read_token(),
        )
        .unwrap();

    let settings = snapshot.settings.unwrap();
    assert_eq!(
        settings.sections,
        vec![SettingsSection::Agents, SettingsSection::CommonSettings]
    );
}

#[test]
fn settings_subscription_uses_backend_owned_settings_collection() {
    let SubscriptionSnapshot::Settings { settings } = builder()
        .snapshot(
            &ctx(),
            &SubscriptionScope::Settings {
                section: Some(SettingsSection::Agents),
            },
            &CursorSequencer::new().read_token(),
        )
        .unwrap()
    else {
        panic!("expected settings snapshot");
    };

    assert_eq!(settings.sections, vec![SettingsSection::Agents]);
}

#[test]
fn client_snapshot_includes_requested_task_for_task_surface() {
    let snapshot = builder()
        .client_snapshot(
            &ctx(),
            RequestedSurface::Task {
                task_id: TaskId::from("task-1"),
            },
            &CursorSequencer::new().read_token(),
        )
        .unwrap();

    let active_task = snapshot.active_task.expect("active task snapshot");
    assert_eq!(active_task.task.task_id.as_str(), "task-1");
    assert_eq!(
        active_task
            .task
            .title
            .as_ref()
            .map(|title| title.value.as_str()),
        Some("Stored task")
    );
}

#[test]
fn task_subscription_uses_backend_owned_task_snapshot() {
    let SubscriptionSnapshot::Task { task } = builder()
        .snapshot(
            &ctx(),
            &SubscriptionScope::Task {
                task_id: TaskId::from("task-1"),
            },
            &CursorSequencer::new().read_token(),
        )
        .unwrap()
    else {
        panic!("expected task snapshot");
    };

    assert_eq!(task.task.task_id.as_str(), "task-1");
    assert_eq!(
        task.task.title.as_ref().map(|title| title.value.as_str()),
        Some("Stored task")
    );
}

fn builder() -> SnapshotBuilder {
    SnapshotBuilder::with_sources(
        "server-1".into(),
        "root-1".into(),
        SnapshotSources::new(
            Arc::new(FixedNewTaskDefaultsForTest),
            Arc::new(AgentRegistrySnapshotSource::new(
                AgentRegistry::default_built_ins(),
            )),
            Arc::new(StaticProjectCollection),
            Arc::new(SettingsCatalog::default()),
            Arc::new(EmptyTaskNavigation),
            Arc::new(StaticTaskSnapshots),
        ),
    )
}

#[derive(Debug)]
struct StaticProjectCollection;

impl ProjectCollectionSnapshotSource for StaticProjectCollection {
    fn snapshot(
        &self,
    ) -> Result<ProjectCollectionSnapshot, openaide_app_server_protocol::errors::ProtocolError>
    {
        Ok(ProjectCollectionSnapshot {
            projects: vec![ProjectSummary {
                project_id: "project-1".into(),
                label: "Project".to_string(),
            }],
        })
    }
}

#[derive(Debug)]
struct FixedNewTaskDefaultsForTest;

impl super::NewTaskDefaultsSnapshotSource for FixedNewTaskDefaultsForTest {
    fn snapshot(
        &self,
    ) -> Result<
        openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Ok(
            openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot {
                project_id: Some("project-1".into()),
                agent_id: Some("codex".into()),
            },
        )
    }
}

#[derive(Debug)]
struct StaticTaskSnapshots;

impl TaskSnapshotSource for StaticTaskSnapshots {
    fn list(
        &self,
        _archived: bool,
        _project_id: Option<&openaide_app_server_protocol::ids::ProjectId>,
        _cursor: Option<&openaide_app_server_protocol::ids::TaskListCursor>,
    ) -> Result<TaskListSnapshot, openaide_app_server_protocol::errors::ProtocolError> {
        Ok(TaskListSnapshot {
            tasks: Vec::new(),
            revision: 0,
            next_cursor: None,
        })
    }

    fn open_internal(
        &self,
        task_id: &TaskId,
    ) -> Result<TaskSnapshot, openaide_app_server_protocol::errors::ProtocolError> {
        Ok(TaskSnapshot {
            task: TaskSummary {
                task_id: task_id.clone(),
                project_id: "project-1".into(),
                agent_id: CODEX_AGENT_ID.into(),
                title: Some(openaide_app_server_protocol::snapshot::TaskTitle {
                    value: "Stored task".to_string(),
                    source: openaide_app_server_protocol::snapshot::TaskTitleSource::User,
                }),
                status: TaskStatus::Idle,
                updated_at: "2026-06-28T00:00:00.000Z".to_string(),
                last_activity: "2026-06-28T00:00:00.000Z".to_string(),
                unread: false,
                has_messages: false,
            },
            lifecycle: openaide_app_server_protocol::snapshot::TaskLifecycle::Visible,
            revision: 7,
            preparation: TaskPreparationSnapshot::Ready,
            agent_config: TaskAgentConfigSnapshot {
                state: LiveSessionDataState::Unavailable,
                options: Vec::new(),
                pending_change: None,
                error: None,
            },
            agent_commands: TaskAgentCommandsSnapshot {
                state: LiveSessionDataState::Unavailable,
                commands: Vec::new(),
                error: None,
            },
            send_capability: TaskSendCapabilitySnapshot {
                state: TaskSendCapabilityState::Blocked,
                blockers: Vec::new(),
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
        })
    }

    fn open_for_client(
        &self,
        _client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<TaskSnapshot, openaide_app_server_protocol::errors::ProtocolError> {
        self.open_internal(task_id)
    }

    fn tool_detail_for_client(
        &self,
        _client_instance_id: &openaide_app_server_protocol::ids::ClientInstanceId,
        _task_id: &TaskId,
        _artifact_id: &str,
    ) -> Result<
        openaide_app_server_protocol::task::ToolDetailSnapshot,
        openaide_app_server_protocol::errors::ProtocolError,
    > {
        Err(openaide_app_server_protocol::errors::ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::NotFound,
            message: "tool detail is not available in this snapshot fixture".to_string(),
            recoverable: false,
            target: None,
        })
    }
}

fn ctx() -> ClientContext {
    ClientContext {
        client_instance_id: ClientInstanceId::from("client-1"),
        connection_id: ConnectionId::new("conn-1"),
        shell: ShellDescriptor {
            kind: ShellKind::Web,
            name: None,
            version: None,
        },
        requested_surface: RequestedSurface::Home,
        capabilities: ClientCapabilities::default(),
    }
}
