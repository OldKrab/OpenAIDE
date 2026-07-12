use openaide_app_server_protocol::errors::ProtocolError;

use crate::protocol::model::{
    ActivityStatus, ActivityStep, InterruptionReason, NormalizedMessage,
    TaskStatus as LegacyTaskStatus,
};
use crate::storage::records::TaskLifecycle;
use crate::task_recovery::RESTART_INTERRUPTION_MESSAGE;
use crate::tasks::mutation::{TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::support::{title_from_prompt, ExistingSend};
use super::TaskProductApi;

impl TaskProductApi {
    pub(super) fn recover_existing_send(
        &self,
        task_id: &str,
        existing: &ExistingSend,
    ) -> Result<(), ProtocolError> {
        if !existing.recovery_required {
            return Ok(());
        }
        let turn_identity = format!("turn:{}", existing.turn_id.as_str());
        let interruption_identity = format!("send-recovery:{}", existing.turn_id.as_str());
        let now = now_string();
        let result = self
            .mutations
            .commit_existing_task(task_id, super::durable_send_commit_options(), |ctx| {
                let messages = self.store.read_messages(task_id)?;
                let turn_status = messages.iter().find_map(|stored| {
                    if stored.chat.identity != turn_identity {
                        return None;
                    }
                    match &stored.chat.message {
                        NormalizedMessage::Activity { status, .. } => Some(*status),
                        _ => None,
                    }
                });
                let task_points_to_turn =
                    ctx.task().active_turn_id.as_deref() == Some(existing.turn_id.as_str());
                let interrupted = match turn_status {
                    None => {
                        ctx.upsert_message(recovered_turn_activity(&turn_identity, &now))?;
                        true
                    }
                    Some(ActivityStatus::Running) if !task_points_to_turn => {
                        ctx.finish_running_activity(&turn_identity, ActivityStatus::Completed)?;
                        true
                    }
                    _ => false,
                };
                if interrupted {
                    ctx.upsert_message(restart_interruption(&interruption_identity, &now))?;
                }

                let task = ctx.task_mut();
                if task.active_turn_id.is_none() || task_points_to_turn {
                    task.status = if turn_status == Some(ActivityStatus::Error) {
                        LegacyTaskStatus::Failed
                    } else {
                        LegacyTaskStatus::Inactive
                    };
                    task.active_turn_id = None;
                }
                task.lifecycle = TaskLifecycle::Visible;
                task.unread |= interrupted;
                task.updated_at = now.clone();
                task.last_activity = now.clone();
                if task.title == "New task" {
                    task.title = title_from_prompt(&existing.fingerprint.text);
                }
                Ok(TaskMutationResult::Changed)
            })
            .map_err(super::super::protocol_error_from_runtime)?;
        if !matches!(result.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(super::conflict_error(
                "Task changed while recovering an interrupted send",
            ));
        }
        Ok(())
    }
}

fn recovered_turn_activity(identity: &str, created_at: &str) -> NormalizedMessage {
    NormalizedMessage::Activity {
        id: identity.to_string(),
        title: "Working".to_string(),
        status: ActivityStatus::Completed,
        created_at: created_at.to_string(),
        collapsed: true,
        steps: vec![ActivityStep::Text {
            text: "Interrupted before Agent start".to_string(),
            level: Some("info".to_string()),
        }],
    }
}

fn restart_interruption(identity: &str, created_at: &str) -> NormalizedMessage {
    NormalizedMessage::Interruption {
        id: identity.to_string(),
        reason: InterruptionReason::Canceled,
        message: RESTART_INTERRUPTION_MESSAGE.to_string(),
        created_at: created_at.to_string(),
        recoverable: true,
    }
}
