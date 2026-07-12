#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChatHistoryPolicy {
    task_snapshot_tail_limit: usize,
}

impl ChatHistoryPolicy {
    pub(crate) fn product_defaults() -> Self {
        Self {
            task_snapshot_tail_limit: 100,
        }
    }

    pub(crate) fn task_snapshot_tail_limit(self) -> usize {
        self.task_snapshot_tail_limit
    }
}

impl Default for ChatHistoryPolicy {
    fn default() -> Self {
        Self::product_defaults()
    }
}

#[cfg(test)]
#[path = "chat_history_tests.rs"]
mod tests;
