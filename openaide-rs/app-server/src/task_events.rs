use std::sync::mpsc;

use openaide_app_server_protocol::events::{TaskChanges, TaskNavigationChange};
use openaide_app_server_protocol::ids::MessageId;
use openaide_app_server_protocol::snapshot::ChatItem;
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
    pub navigation: Option<TaskNavigationChange>,
    pub lifecycle: Option<openaide_app_server_protocol::task::TaskLifecycleChanged>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskUpdateKind {
    Changed(Box<CommittedTaskChange>),
    HistorySync(openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot),
    ToolDetailChanged {
        artifact_id: String,
        deltas: Vec<openaide_app_server_protocol::events::ToolDetailDelta>,
    },
    /// The global Task Navigation projection changed outside a Task transaction.
    NavigationChanged,
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
}

pub type TaskUpdateReceiver = mpsc::Receiver<TaskUpdate>;

impl TaskUpdateNotifier {
    pub fn disabled() -> Self {
        Self { sender: None }
    }

    pub fn channel() -> (Self, TaskUpdateReceiver) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                sender: Some(sender),
            },
            receiver,
        )
    }

    pub(crate) fn task_changed(&self, task_id: &str, revision: u64, change: CommittedTaskChange) {
        self.publish(TaskUpdate {
            task_id: task_id.to_string(),
            revision,
            kind: TaskUpdateKind::Changed(Box::new(change)),
        });
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

    pub(crate) fn navigation_changed(&self) {
        self.publish(TaskUpdate {
            task_id: String::new(),
            revision: 0,
            kind: TaskUpdateKind::NavigationChanged,
        });
    }

    fn publish(&self, update: TaskUpdate) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(update);
        }
    }
}
