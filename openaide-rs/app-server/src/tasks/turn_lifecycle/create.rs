use crate::tasks::mutation::TaskCommitOptions;

use super::TaskTurnLifecycle;

mod adopted_session;
mod helpers;
mod prompt_start;

fn create_snapshot_commit_options() -> TaskCommitOptions {
    TaskCommitOptions {
        refresh_message_history: false,
        response_snapshot_tail_limit: Some(100),
    }
}
