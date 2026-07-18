use std::sync::mpsc;

use openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot;

/// Publishes repository projections produced by background Git operations.
#[derive(Clone, Default)]
pub struct WorktreeUpdateNotifier {
    sender: Option<mpsc::Sender<WorktreeRepositorySnapshot>>,
}

pub type WorktreeUpdateReceiver = mpsc::Receiver<WorktreeRepositorySnapshot>;

impl WorktreeUpdateNotifier {
    pub fn disabled() -> Self {
        Self { sender: None }
    }

    pub fn channel() -> (Self, WorktreeUpdateReceiver) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                sender: Some(sender),
            },
            receiver,
        )
    }

    pub(crate) fn repository_updated(&self, repository: WorktreeRepositorySnapshot) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(repository);
        }
    }
}
