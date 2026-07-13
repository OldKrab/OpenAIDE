#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
#[cfg(test)]
use std::sync::{Arc, Barrier};

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskFieldChanges {
    pub summary: bool,
    pub lifecycle: bool,
    pub preparation: bool,
    pub agent_config: bool,
    pub agent_commands: bool,
    pub send_capability: bool,
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskNavigationChange {
    None,
    Upsert,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDetailUpdate {
    pub artifact_id: String,
    pub details: ToolDetailSnapshot,
}

/// The complete focused publication produced by one durable Task transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedTaskChange {
    pub fields: TaskFieldChanges,
    pub chat: Vec<CommittedChatChange>,
    pub tool_details: Vec<ToolDetailUpdate>,
    pub navigation: TaskNavigationChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskUpdateKind {
    Changed(CommittedTaskChange),
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
    #[cfg(test)]
    blocking_once: Option<Arc<BlockingTaskUpdateInner>>,
}

pub type TaskUpdateReceiver = mpsc::Receiver<TaskUpdate>;

impl TaskUpdateNotifier {
    pub fn disabled() -> Self {
        Self {
            sender: None,
            #[cfg(test)]
            blocking_once: None,
        }
    }

    pub fn channel() -> (Self, TaskUpdateReceiver) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                sender: Some(sender),
                #[cfg(test)]
                blocking_once: None,
            },
            receiver,
        )
    }

    pub(crate) fn task_changed(&self, task_id: &str, revision: u64, change: CommittedTaskChange) {
        self.publish(TaskUpdate {
            task_id: task_id.to_string(),
            revision,
            kind: TaskUpdateKind::Changed(change),
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
        #[cfg(test)]
        if let Some(blocking) = &self.blocking_once {
            if blocking.armed.swap(false, Ordering::SeqCst) {
                blocking.reached.wait();
                blocking.release.wait();
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn blocking_once_for_test() -> (Self, BlockingTaskUpdate) {
        let inner = Arc::new(BlockingTaskUpdateInner {
            armed: AtomicBool::new(true),
            reached: Barrier::new(2),
            release: Barrier::new(2),
        });
        (
            Self {
                sender: None,
                blocking_once: Some(inner.clone()),
            },
            BlockingTaskUpdate { inner },
        )
    }
}

#[cfg(test)]
struct BlockingTaskUpdateInner {
    armed: AtomicBool,
    reached: Barrier,
    release: Barrier,
}

#[cfg(test)]
pub(crate) struct BlockingTaskUpdate {
    inner: Arc<BlockingTaskUpdateInner>,
}

#[cfg(test)]
impl BlockingTaskUpdate {
    pub(crate) fn wait_until_blocked(&self) {
        self.inner.reached.wait();
    }

    pub(crate) fn release(&self) {
        self.inner.release.wait();
    }
}
