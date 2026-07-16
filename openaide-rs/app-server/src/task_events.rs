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
}

/// The complete focused publication produced by one durable Task transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedTaskChange {
    /// Exact values captured by the durable transaction, never re-read during publication.
    pub changes: TaskChanges,
    pub tool_details: Vec<ToolDetailUpdate>,
    pub navigation: Option<TaskNavigationChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskUpdateKind {
    Changed(Box<CommittedTaskChange>),
    HistorySync(openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot),
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

    fn publish(&self, update: TaskUpdate) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(update);
        }
    }
}
