#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
#[cfg(test)]
use std::sync::{Arc, Barrier};

use openaide_app_server_protocol::events::TextChunk;
use openaide_app_server_protocol::ids::MessageId;
use openaide_app_server_protocol::snapshot::ChatItem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommittedTaskDelta {
    ChatItemAppended {
        item: ChatItem,
    },
    ChatItemChunk {
        message_id: MessageId,
        chunk: TextChunk,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskUpdate {
    pub task_id: String,
    pub revision: u64,
    pub delta: Option<CommittedTaskDelta>,
    pub history_sync: Option<openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot>,
}

impl TaskUpdate {
    pub fn committed(task_id: impl Into<String>, revision: u64, delta: CommittedTaskDelta) -> Self {
        Self {
            task_id: task_id.into(),
            revision,
            delta: Some(delta),
            history_sync: None,
        }
    }
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

    pub fn task_updated(&self, task_id: &str, revision: u64) {
        self.publish(TaskUpdate {
            task_id: task_id.to_string(),
            revision,
            delta: None,
            history_sync: None,
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
            delta: None,
            history_sync: Some(history_sync),
        });
    }

    pub(crate) fn task_updated_with_delta(
        &self,
        task_id: &str,
        revision: u64,
        delta: CommittedTaskDelta,
    ) {
        self.publish(TaskUpdate::committed(task_id, revision, delta));
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
