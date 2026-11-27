use std::sync::mpsc;

use openaide_app_server_protocol::events::TaskChanges;
use openaide_app_server_protocol::ids::{MessageId, ProjectId};
use openaide_app_server_protocol::snapshot::ChatItem;

use crate::navigation::NavigationDirty;
use openaide_app_server_protocol::task::ToolDetailSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommittedChatChange {
    Append { item: ChatItem },
    Upsert { item: ChatItem },
    AppendText { message_id: MessageId, text: String },
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDetailUpdate {
    pub artifact_id: String,
    pub details: ToolDetailSnapshot,
    pub terminal_appends: Vec<crate::storage::task_journal::TerminalOutputAppend>,
}

/// The complete focused publication produced by one durable Task transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedTaskChange {
    /// Exact values captured by the durable transaction, never re-read during publication.
    pub changes: TaskChanges,
    pub tool_details: Vec<ToolDetailUpdate>,
    /// The Project whose Navigation rows this commit changed (row fields, membership,
    /// lifecycle or order). The notifier marks it dirty when the commit is announced.
    pub navigation: Option<ProjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskUpdateKind {
    Changed(Box<CommittedTaskChange>),
    HistorySync(openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot),
    ToolDetailChanged {
        artifact_id: String,
        deltas: Vec<openaide_app_server_protocol::events::ToolDetailDelta>,
    },
    SubagentCatalogChanged {
        catalog: openaide_app_server_protocol::snapshot::SubagentCatalogSnapshot,
    },
    /// One committed Subagent History revision; `TaskUpdate::revision` is its revision.
    SubagentHistoryChanged {
        subagent_id: openaide_app_server_protocol::ids::SubagentId,
        chat: Vec<openaide_app_server_protocol::events::TaskChatChange>,
        current_plan: Option<Option<openaide_app_server_protocol::snapshot::AgentPlanSnapshot>>,
    },
    /// Navigation was marked dirty. The update only wakes the publisher, which flushes.
    NavigationDirty,
}

impl TaskUpdateKind {
    /// Stable metadata-only name for diagnostics.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Changed(_) => "changed",
            Self::HistorySync(_) => "history_sync",
            Self::ToolDetailChanged { .. } => "tool_detail_changed",
            Self::SubagentCatalogChanged { .. } => "subagent_catalog_changed",
            Self::SubagentHistoryChanged { .. } => "subagent_history_changed",
            Self::NavigationDirty => "navigation_dirty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskUpdate {
    pub task_id: String,
    pub revision: u64,
    pub kind: TaskUpdateKind,
}

#[derive(Clone, Default)]
pub struct TaskUpdateNotifier {
    sender: Option<mpsc::Sender<TaskUpdate>>,
    navigation_dirty: NavigationDirty,
}

pub type TaskUpdateReceiver = mpsc::Receiver<TaskUpdate>;

impl TaskUpdateNotifier {
    pub fn disabled() -> Self {
        Self {
            sender: None,
            navigation_dirty: NavigationDirty::default(),
        }
    }

    pub fn channel() -> (Self, TaskUpdateReceiver) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                sender: Some(sender),
                navigation_dirty: NavigationDirty::default(),
            },
            receiver,
        )
    }

    /// The marks the Navigation owner consumes.
    pub(crate) fn navigation_dirty(&self) -> NavigationDirty {
        self.navigation_dirty.clone()
    }

    pub(crate) fn task_changed(&self, task_id: &str, revision: u64, change: CommittedTaskChange) {
        // The mark is set at the commit, before anything else can observe it. The Task's own
        // update is the wake: the publisher takes the protocol lock for it, and releasing the
        // lock flushes the mark.
        if let Some(project_id) = &change.navigation {
            self.navigation_dirty.mark_project(project_id.clone());
        }
        self.publish(TaskUpdate {
            task_id: task_id.to_string(),
            revision,
            kind: TaskUpdateKind::Changed(Box::new(change)),
        });
    }

    pub(crate) fn streamed_chat_changed(
        &self,
        task_id: &str,
        revision: u64,
        chat: Vec<openaide_app_server_protocol::events::TaskChatChange>,
    ) {
        self.task_changed(
            task_id,
            revision,
            CommittedTaskChange {
                changes: TaskChanges {
                    chat,
                    ..Default::default()
                },
                tool_details: Vec::new(),
                navigation: None,
            },
        );
    }

    pub(crate) fn history_sync_updated(
        &self,
        task_id: &str,
        revision: u64,
        history_sync: openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot,
    ) {
        self.publish(TaskUpdate {
            task_id: task_id.to_string(),
            revision,
            kind: TaskUpdateKind::HistorySync(history_sync),
        });
    }

    pub(crate) fn tool_detail_changed(
        &self,
        task_id: &str,
        artifact_id: String,
        artifact_sequence: u64,
        appends: Vec<crate::storage::task_journal::TerminalOutputAppend>,
    ) {
        if appends.is_empty() {
            return;
        }
        self.publish(TaskUpdate {
            task_id: task_id.to_string(),
            revision: artifact_sequence,
            kind: TaskUpdateKind::ToolDetailChanged {
                artifact_id,
                deltas: appends
                    .into_iter()
                    .map(|append| {
                        openaide_app_server_protocol::events::ToolDetailDelta::AppendTerminal {
                            terminal_id: append.terminal_id,
                            data: append.data,
                        }
                    })
                    .collect(),
            },
        });
    }

    pub(crate) fn subagent_catalog_changed(
        &self,
        catalog: openaide_app_server_protocol::snapshot::SubagentCatalogSnapshot,
    ) {
        self.publish(TaskUpdate {
            task_id: catalog.task_id.as_str().to_string(),
            revision: catalog.revision,
            kind: TaskUpdateKind::SubagentCatalogChanged { catalog },
        });
    }

    pub(crate) fn subagent_history_changed(
        &self,
        task_id: &str,
        subagent_id: &str,
        revision: u64,
        chat: Vec<openaide_app_server_protocol::events::TaskChatChange>,
        current_plan: Option<Option<openaide_app_server_protocol::snapshot::AgentPlanSnapshot>>,
    ) {
        self.publish(TaskUpdate {
            task_id: task_id.to_string(),
            revision,
            kind: TaskUpdateKind::SubagentHistoryChanged {
                subagent_id: subagent_id.into(),
                chat,
                current_plan,
            },
        });
    }

    /// Marks one Project's Navigation rows or discovery state changed. Called at the commit
    /// or state change itself, so a handler's reply already follows the marks it made.
    pub(crate) fn navigation_project_changed(&self, project_id: impl Into<ProjectId>) {
        if self.navigation_dirty.mark_project(project_id.into()) {
            self.wake_navigation();
        }
    }

    fn wake_navigation(&self) {
        self.publish(TaskUpdate {
            task_id: String::new(),
            revision: 0,
            kind: TaskUpdateKind::NavigationDirty,
        });
    }

    fn publish(&self, update: TaskUpdate) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(update);
        }
    }
}
