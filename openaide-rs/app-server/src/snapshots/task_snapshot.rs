use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId, TaskId, TaskListCursor};
use openaide_app_server_protocol::snapshot::{
    ChatSnapshot, SubagentCapabilitiesSnapshot, SubagentCatalogEntrySnapshot,
    SubagentCatalogSnapshot, SubagentDetailSnapshot, SubagentHistoryAvailability,
    SubagentHistorySnapshot, SubagentOverviewSnapshot, SubagentStatus, TaskHistorySyncSnapshot,
    TaskSnapshot, TaskSummary,
};
use openaide_app_server_protocol::task::{TaskListLifecycle, ToolDetailSnapshot, ToolImagePreview};
use std::sync::Arc;

use crate::chat_history::ChatHistoryPolicy;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::MessagePage;
use crate::protocol::model::TaskSnapshot as StoredTaskSnapshot;
use crate::storage::records::TaskRecord;
use crate::storage::Store;
use crate::tasks::snapshot::{build_snapshot, snapshot_from_record_and_chat};

pub(crate) use chat_projection::{project_chat_item, project_tool_details};

mod tool_image_preview;

/// Projects one committed lazy artifact into the complete replica baseline.
/// Structured details and terminal output share the artifact revision and must
/// never be published as competing partial snapshots.
pub(crate) fn project_tool_artifact(
    artifact: crate::storage::task_journal::ToolArtifactProjection,
) -> Option<ToolDetailSnapshot> {
    let details = artifact.details?;
    let mut snapshot = project_tool_details(&details);
    snapshot.revision = artifact.revision;
    snapshot.terminal_outputs = artifact
        .terminal_order
        .into_iter()
        .filter_map(|terminal_id| {
            artifact.terminal_outputs.get(&terminal_id).map(|output| {
                openaide_app_server_protocol::task::TerminalOutputSnapshot {
                    terminal_id,
                    output: output.clone(),
                }
            })
        })
        .collect();
    Some(snapshot)
}
use readiness::{
    agent_commands_snapshot, agent_config_snapshot, preparation_snapshot, send_capability_for_task,
};

use super::task_navigation::{
    project_legacy_task_summary, project_status_with_preparation, project_task_summary,
    snapshot_read_error,
};

mod chat_projection;
mod readiness;

pub trait TaskSnapshotSource: Send + Sync {
    fn list(
        &self,
        lifecycle: TaskListLifecycle,
        project_id: Option<&ProjectId>,
        cursor: Option<&TaskListCursor>,
    ) -> Result<TaskListSnapshot, ProtocolError>;

    /// Reads a Task for internal publication after its audience is already established.
    fn open_internal(&self, task_id: &TaskId) -> Result<TaskSnapshot, ProtocolError>;

    /// Reads a Task on behalf of a client, enforcing New Task ownership.
    fn open_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<TaskSnapshot, ProtocolError>;

    /// Reads one expanded Tool detail after enforcing Task access for the client.
    fn tool_detail_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        artifact_id: &str,
    ) -> Result<ToolDetailSnapshot, ProtocolError>;

    fn subagent_catalog_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
    ) -> Result<SubagentCatalogSnapshot, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message: "Subagent catalog is unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn subagent_history_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
        _subagent_id: &openaide_app_server_protocol::ids::SubagentId,
    ) -> Result<SubagentHistorySnapshot, ProtocolError> {
        Err(ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message: "Subagent history is unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }

    /// Resolves an image from server-owned Tool detail paths without accepting a client path.
    fn tool_image_preview_for_client(
        &self,
        _client_instance_id: &ClientInstanceId,
        _task_id: &TaskId,
        _artifact_id: &str,
    ) -> Result<Option<ToolImagePreview>, ProtocolError> {
        Ok(None)
    }

    /// Resolves the Task Workspace for a File Viewer open without exposing a client path authority.
    fn workspace_root_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<String, ProtocolError> {
        let _ = (client_instance_id, task_id);
        Err(ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message: "File Viewer is unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }
}

/// Supplies process-local history reconciliation state for otherwise durable Task snapshots.
pub(crate) trait TaskHistorySyncSnapshotSource: Send + Sync {
    fn history_sync_snapshot(&self, task_id: &str) -> TaskHistorySyncSnapshot;
}

#[derive(Default)]
struct IdleTaskHistorySyncSnapshots;

impl TaskHistorySyncSnapshotSource for IdleTaskHistorySyncSnapshots {
    fn history_sync_snapshot(&self, _task_id: &str) -> TaskHistorySyncSnapshot {
        TaskHistorySyncSnapshot::default()
    }
}

#[derive(Debug, Clone)]
pub struct TaskListSnapshot {
    pub tasks: Vec<TaskSummary>,
    pub revision: u64,
    pub next_cursor: Option<TaskListCursor>,
}

#[derive(Clone)]
pub struct TaskSnapshotStore {
    store: Store,
    tail_limit: usize,
    history_sync: Arc<dyn TaskHistorySyncSnapshotSource>,
}

impl TaskSnapshotStore {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            tail_limit: ChatHistoryPolicy::default().task_snapshot_tail_limit(),
            history_sync: Arc::new(IdleTaskHistorySyncSnapshots),
        }
    }

    pub(crate) fn with_history_sync(
        store: Store,
        history_sync: Arc<dyn TaskHistorySyncSnapshotSource>,
    ) -> Self {
        Self {
            store,
            tail_limit: ChatHistoryPolicy::default().task_snapshot_tail_limit(),
            history_sync,
        }
    }
}

impl TaskSnapshotSource for TaskSnapshotStore {
    fn list(
        &self,
        lifecycle: TaskListLifecycle,
        project_id: Option<&ProjectId>,
        cursor: Option<&TaskListCursor>,
    ) -> Result<TaskListSnapshot, ProtocolError> {
        if cursor.is_some() {
            return Err(unsupported_cursor_error());
        }
        let records = match lifecycle {
            TaskListLifecycle::Open => self.store.list_tasks(),
            TaskListLifecycle::Archived => self.store.list_archived_tasks(),
        };
        let tasks = records
            .map_err(snapshot_read_error)?
            .into_iter()
            .map(project_task_summary)
            .filter(|task| project_id.is_none_or(|project_id| &task.project_id == project_id))
            .collect();
        let revision = self
            .store
            .max_listed_task_revision()
            .map_err(snapshot_read_error)?;
        Ok(TaskListSnapshot {
            tasks,
            revision,
            next_cursor: None,
        })
    }

    fn open_internal(&self, task_id: &TaskId) -> Result<TaskSnapshot, ProtocolError> {
        self.open_authorized(task_id, |_| Ok(()))
    }

    fn open_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<TaskSnapshot, ProtocolError> {
        self.open_authorized(task_id, |task| {
            crate::tasks::access::require_client_task_access(task, client_instance_id)
        })
    }

    fn tool_detail_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        artifact_id: &str,
    ) -> Result<ToolDetailSnapshot, ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        crate::tasks::access::require_client_task_access(&task, client_instance_id)
            .map_err(task_snapshot_error)?;
        let artifact = self
            .store
            .read_tool_artifact_projection(task_id.as_str(), artifact_id)
            .map_err(task_snapshot_error)?;
        project_tool_artifact(artifact).ok_or_else(|| ProtocolError {
            code: openaide_app_server_protocol::errors::ProtocolErrorCode::NotFound,
            message: "Tool detail baseline is unavailable".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn subagent_catalog_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<SubagentCatalogSnapshot, ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        crate::tasks::access::require_client_task_access(&task, client_instance_id)
            .map_err(task_snapshot_error)?;
        project_subagent_catalog(
            task_id.clone(),
            self.store.subagent_catalog(task_id.as_str()),
        )
    }

    fn subagent_history_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        subagent_id: &openaide_app_server_protocol::ids::SubagentId,
    ) -> Result<SubagentHistorySnapshot, ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        crate::tasks::access::require_client_task_access(&task, client_instance_id)
            .map_err(task_snapshot_error)?;
        let catalog = self
            .store
            .subagent_catalog(task_id.as_str())
            .map_err(task_snapshot_error)?;
        let Some(record) = catalog
            .entries
            .iter()
            .find(|entry| entry.subagent_id == subagent_id.as_str())
        else {
            return Err(ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: "Subagent is unavailable".to_string(),
                recoverable: false,
                target: None,
            });
        };
        let history = self
            .store
            .subagent_history(task_id.as_str(), subagent_id.as_str());
        Ok(match history {
            Ok(history) => {
                let tail_limit = self.tail_limit;
                let start = history.messages.len().saturating_sub(tail_limit);
                let retained = &history.messages[start..];
                SubagentHistorySnapshot {
                    task_id: task_id.clone(),
                    subagent_id: subagent_id.clone(),
                    revision: history.revision,
                    availability: if history.messages.is_empty()
                        && matches!(
                            record.status,
                            crate::storage::subagents::SubagentStatusRecord::WaitingForActivity
                        ) {
                        SubagentHistoryAvailability::WaitingForActivity
                    } else {
                        SubagentHistoryAvailability::Available
                    },
                    chat: ChatSnapshot {
                        items: retained
                            .iter()
                            .map(|stored| project_chat_item(&stored.chat))
                            .collect(),
                        has_more_before: start > 0,
                        has_messages: !history.messages.is_empty(),
                        start_cursor: retained
                            .first()
                            .map(|stored| stored.chat.cursor.clone().into()),
                        end_cursor: retained
                            .last()
                            .map(|stored| stored.chat.cursor.clone().into()),
                    },
                    current_plan: history.current_plan.map(project_agent_plan),
                    start_cursor: retained
                        .first()
                        .map(|stored| stored.chat.cursor.clone().into()),
                }
            }
            Err(_) => SubagentHistorySnapshot {
                task_id: task_id.clone(),
                subagent_id: subagent_id.clone(),
                revision: record.history_revision,
                availability: SubagentHistoryAvailability::Unavailable,
                chat: ChatSnapshot {
                    items: Vec::new(),
                    has_more_before: false,
                    has_messages: false,
                    start_cursor: None,
                    end_cursor: None,
                },
                current_plan: None,
                start_cursor: None,
            },
        })
    }

    fn tool_image_preview_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        artifact_id: &str,
    ) -> Result<Option<ToolImagePreview>, ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        crate::tasks::access::require_client_task_access(&task, client_instance_id)
            .map_err(task_snapshot_error)?;
        let details = self
            .store
            .read_tool_artifact(task_id.as_str(), artifact_id)
            .map_err(task_snapshot_error)?;
        Ok(tool_image_preview::load_tool_image_preview(&task, &details))
    }

    fn workspace_root_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<String, ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        crate::tasks::access::require_client_task_access(&task, client_instance_id)
            .map_err(task_snapshot_error)?;
        Ok(task.workspace_root)
    }
}

impl TaskSnapshotStore {
    fn open_authorized(
        &self,
        task_id: &TaskId,
        authorize: impl FnOnce(
            &crate::storage::records::TaskRecord,
        ) -> Result<(), crate::protocol::errors::RuntimeError>,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        if task.tombstoned {
            return Err(ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: format!("task not found: {}", task_id.as_str()),
                recoverable: false,
                target: None,
            });
        }
        authorize(&task).map_err(task_snapshot_error)?;
        let snapshot = build_snapshot(&self.store, task_id.as_str(), self.tail_limit)
            .map_err(task_snapshot_error)?;
        let history_sync = self.history_sync.history_sync_snapshot(task_id.as_str());
        let mut projected = project_stored_task_snapshot_with_history_sync(snapshot, history_sync)?;
        projected.subagents = subagent_overview(&self.store, task_id.as_str());
        Ok(projected)
    }
}

#[cfg(test)]
pub(crate) fn project_stored_task_snapshot(
    snapshot: StoredTaskSnapshot,
) -> Result<TaskSnapshot, ProtocolError> {
    project_stored_task_snapshot_with_history_sync(snapshot, TaskHistorySyncSnapshot::default())
}

pub(crate) fn project_stored_task_snapshot_with_history_sync(
    snapshot: StoredTaskSnapshot,
    history_sync: TaskHistorySyncSnapshot,
) -> Result<TaskSnapshot, ProtocolError> {
    let history_sync = match (
        history_sync,
        snapshot.native_session_reload_requirement.as_ref(),
    ) {
        (TaskHistorySyncSnapshot::Idle { generation }, Some(_)) => {
            TaskHistorySyncSnapshot::ReloadAvailable { generation }
        }
        (history_sync, _) => history_sync,
    };
    let lifecycle = match snapshot.lifecycle {
        crate::storage::records::TaskLifecycle::Prepared { .. } => {
            openaide_app_server_protocol::snapshot::TaskLifecycle::Prepared
        }
        crate::storage::records::TaskLifecycle::Open => {
            openaide_app_server_protocol::snapshot::TaskLifecycle::Open
        }
        crate::storage::records::TaskLifecycle::Archived => {
            openaide_app_server_protocol::snapshot::TaskLifecycle::Archived
        }
    };
    let send_capability = send_capability_for_task(snapshot.task.status, &snapshot.preparation);
    let agent_config = agent_config_snapshot(&snapshot);
    let agent_commands = agent_commands_snapshot(&snapshot);
    let projected_status =
        project_status_with_preparation(snapshot.task.status, &snapshot.preparation);
    let mut task =
        project_legacy_task_summary(snapshot.task, snapshot.chat.total_count > 0, lifecycle);
    task.status = projected_status;
    Ok(TaskSnapshot {
        task,
        permission_policy: snapshot.permission_policy,
        active_turn_started_at: snapshot.active_turn_started_at,
        lifecycle,
        revision: snapshot.revision,
        preparation: preparation_snapshot(&snapshot.preparation),
        agent_config,
        agent_commands,
        send_capability,
        input_capabilities: Some(
            openaide_app_server_protocol::snapshot::TaskInputCapabilities {
                image: snapshot.supports_image_input,
            },
        ),
        context_usage: snapshot.context_usage.map(|usage| {
            openaide_app_server_protocol::snapshot::TaskContextUsage {
                used_tokens: usage.used_tokens,
                capacity_tokens: usage.capacity_tokens,
                cost: usage.cost.map(|cost| {
                    openaide_app_server_protocol::snapshot::TaskUsageCost {
                        amount: cost.amount,
                        currency: cost.currency,
                    }
                }),
                last_turn: usage.last_turn.map(|turn| {
                    openaide_app_server_protocol::snapshot::TaskTurnUsage {
                        total_tokens: turn.total_tokens,
                        input_tokens: turn.input_tokens,
                        output_tokens: turn.output_tokens,
                        reasoning_tokens: turn.reasoning_tokens,
                        cached_read_tokens: turn.cached_read_tokens,
                        cached_write_tokens: turn.cached_write_tokens,
                    }
                }),
            }
        }),
        current_plan: snapshot.current_plan.map(project_agent_plan),
        message_queue: openaide_app_server_protocol::snapshot::TaskMessageQueueSnapshot {
            revision: snapshot.message_queue.revision,
            pause: snapshot.message_queue.pause.map(|pause| match pause {
                crate::storage::records::TaskMessageQueuePauseRecord::Restarted =>
                    openaide_app_server_protocol::snapshot::TaskMessageQueuePauseSnapshot::Restarted,
                crate::storage::records::TaskMessageQueuePauseRecord::UnsuccessfulTurn =>
                    openaide_app_server_protocol::snapshot::TaskMessageQueuePauseSnapshot::UnsuccessfulTurn,
                crate::storage::records::TaskMessageQueuePauseRecord::AttachmentUnavailable =>
                    openaide_app_server_protocol::snapshot::TaskMessageQueuePauseSnapshot::AttachmentUnavailable,
            }),
            items: snapshot
                .message_queue
                .items
                .into_iter()
                .map(
                    |item| openaide_app_server_protocol::snapshot::QueuedMessageSnapshot {
                        queued_message_id: item.queued_message_id.into(),
                        text: item.text,
                        created_at: item.created_at,
                        attachments: item.chat_attachments.into_iter().map(|attachment| {
                            openaide_app_server_protocol::snapshot::QueuedMessageAttachmentSnapshot {
                                kind: attachment.kind,
                                label: attachment.label,
                            }
                        }).collect(),
                    },
                )
                .collect(),
        },
        subagents: SubagentOverviewSnapshot::default(),
        chat: project_chat_page(snapshot.chat),
        history_sync,
        pending_requests: Vec::new(),
        recovery: None,
    })
}

pub(crate) fn project_subagent_catalog(
    task_id: TaskId,
    catalog: Result<crate::storage::subagents::SubagentCatalogProjection, RuntimeError>,
) -> Result<SubagentCatalogSnapshot, ProtocolError> {
    let catalog = catalog.map_err(task_snapshot_error)?;
    Ok(SubagentCatalogSnapshot {
        task_id,
        revision: catalog.revision,
        entries: catalog
            .entries
            .into_iter()
            .map(|entry| SubagentCatalogEntrySnapshot {
                subagent_id: entry.subagent_id.into(),
                parent_subagent_id: entry.parent_subagent_id.map(Into::into),
                name: entry.name,
                delegated_task: entry.delegated_task,
                status: project_subagent_status(entry.status),
                capabilities: SubagentCapabilitiesSnapshot {
                    cancel: entry.capabilities.cancel,
                    close: entry.capabilities.close,
                },
                spawned_order: entry.spawned_order,
                history_revision: entry.history_revision,
                history_available: entry.history_available,
                details: entry
                    .details
                    .into_iter()
                    .map(|detail| SubagentDetailSnapshot {
                        label: detail.label,
                        value: detail.value,
                    })
                    .collect(),
            })
            .collect(),
        has_more: false,
    })
}

fn project_subagent_status(
    status: crate::storage::subagents::SubagentStatusRecord,
) -> SubagentStatus {
    use crate::storage::subagents::SubagentStatusRecord as Stored;
    match status {
        Stored::WaitingForActivity => SubagentStatus::WaitingForActivity,
        Stored::Running => SubagentStatus::Running,
        Stored::Completed => SubagentStatus::Completed,
        Stored::Failed => SubagentStatus::Failed,
        Stored::Cancelled => SubagentStatus::Cancelled,
        Stored::Disconnected => SubagentStatus::Disconnected,
    }
}

pub(crate) fn subagent_overview(store: &Store, task_id: &str) -> SubagentOverviewSnapshot {
    match store.subagent_catalog(task_id) {
        Ok(catalog) => SubagentOverviewSnapshot {
            total_count: catalog.entries.len() as u64,
            running_count: catalog
                .entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.status,
                        crate::storage::subagents::SubagentStatusRecord::WaitingForActivity
                            | crate::storage::subagents::SubagentStatusRecord::Running
                    )
                })
                .count() as u64,
            attention_count: catalog
                .entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.status,
                        crate::storage::subagents::SubagentStatusRecord::Failed
                    ) || !entry.history_available
                })
                .count() as u64,
            available: true,
        },
        Err(_) => SubagentOverviewSnapshot {
            available: false,
            ..Default::default()
        },
    }
}

pub(crate) fn project_agent_plan(
    plan: crate::protocol::model::AgentPlan,
) -> openaide_app_server_protocol::snapshot::AgentPlanSnapshot {
    use crate::protocol::model::{AgentPlanPriority, AgentPlanStatus};
    use openaide_app_server_protocol::snapshot::{
        AgentPlanEntrySnapshot, AgentPlanPrioritySnapshot, AgentPlanSnapshot,
        AgentPlanStatusSnapshot,
    };

    AgentPlanSnapshot {
        entries: plan
            .entries
            .into_iter()
            .map(|entry| AgentPlanEntrySnapshot {
                content: entry.content,
                priority: match entry.priority {
                    AgentPlanPriority::High => AgentPlanPrioritySnapshot::High,
                    AgentPlanPriority::Medium => AgentPlanPrioritySnapshot::Medium,
                    AgentPlanPriority::Low => AgentPlanPrioritySnapshot::Low,
                },
                status: match entry.status {
                    AgentPlanStatus::Pending => AgentPlanStatusSnapshot::Pending,
                    AgentPlanStatus::InProgress => AgentPlanStatusSnapshot::InProgress,
                    AgentPlanStatus::Completed => AgentPlanStatusSnapshot::Completed,
                },
            })
            .collect(),
    }
}

/// Projects metadata from the exact committed Task record without reading Chat from storage.
pub(crate) fn project_committed_task_state(
    task: TaskRecord,
    has_messages: bool,
) -> Result<TaskSnapshot, ProtocolError> {
    let task_id = task.task_id.clone();
    let version = task.message_history_version;
    project_stored_task_snapshot_with_history_sync(
        snapshot_from_record_and_chat(
            task,
            MessagePage {
                task_id,
                items: Vec::new(),
                has_before: false,
                total_count: u64::from(has_messages),
                version,
                start_cursor: None,
                end_cursor: None,
            },
        ),
        TaskHistorySyncSnapshot::default(),
    )
}

/// Projects a Chat page captured under the same mutation lock as its Task revision.
pub(crate) fn project_chat_page(chat: MessagePage) -> ChatSnapshot {
    ChatSnapshot {
        items: chat.items.iter().map(project_chat_item).collect(),
        has_more_before: chat.has_before,
        has_messages: chat.total_count > 0,
        start_cursor: chat.start_cursor.map(Into::into),
        end_cursor: chat.end_cursor.map(Into::into),
    }
}

fn unsupported_cursor_error() -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::CapabilityUnavailable,
        message: "Task list pagination is not available in this API slice".to_string(),
        recoverable: true,
        target: None,
    }
}

fn task_snapshot_error(error: impl std::fmt::Display) -> ProtocolError {
    let message = error.to_string();
    let code = if message.starts_with("task not found") {
        ProtocolErrorCode::NotFound
    } else {
        ProtocolErrorCode::Internal
    };
    ProtocolError {
        code,
        message,
        recoverable: code != ProtocolErrorCode::NotFound,
        target: None,
    }
}

#[cfg(test)]
mod tests;
