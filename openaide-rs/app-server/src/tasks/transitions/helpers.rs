use uuid::Uuid;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{InterruptionReason, NormalizedMessage};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationContext};

pub(super) fn append_interruption(
    ctx: &TaskMutationContext<'_>,
    reason: InterruptionReason,
    message: &str,
    created_at: String,
    recoverable: bool,
) -> Result<(), RuntimeError> {
    ctx.append_message(NormalizedMessage::Interruption {
        id: Uuid::new_v4().to_string(),
        reason,
        message: message.to_string(),
        created_at,
        recoverable,
    })
}

pub(super) fn chat_commit_options() -> TaskCommitOptions {
    TaskCommitOptions {
        refresh_message_history: true,
        response_snapshot_tail_limit: None,
    }
}
