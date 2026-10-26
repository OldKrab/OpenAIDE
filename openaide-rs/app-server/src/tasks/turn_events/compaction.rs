use crate::agent::acp_compaction_projection::apply_compaction_change;
use crate::agent::acp_message_identity::stable_compaction_id;
use crate::agent::events::AgentCompactionChange;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus;
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};

use super::TaskSessionEventSink;

impl TaskSessionEventSink {
    /// Commits one ordered compaction change as an in-place upsert of the row
    /// keyed by the Agent's compaction id. Reading the stored row and writing
    /// the patched one happen in one task commit, so concurrent chunks cannot
    /// lose each other's text.
    pub(super) fn update_compaction(
        &self,
        compaction_id: &str,
        change: AgentCompactionChange,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let id = stable_compaction_id(&self.session_id, compaction_id);
        // Summary chunks are high-frequency and carry content, so only status updates are logged.
        let status = match &change {
            AgentCompactionChange::Update { status, .. } => Some(*status),
            AgentCompactionChange::SummaryChunk { .. } => None,
        };
        let mut applied = None;
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    applied = Some(("other_session", false));
                    return Ok(TaskMutationResult::Unchanged);
                }
                // Whether a turn was open says if the Task's working state and this row agree.
                let turn_open =
                    ctx.task().status == TaskStatus::Active && ctx.task().active_turn_id.is_some();
                applied = Some(("applied", turn_open));
                let message =
                    apply_compaction_change(&id, ctx.message_by_identity(&id), change, now);
                ctx.upsert_message_with_details(message)?;
                ctx.task_mut().updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if let (Some(status), Some((outcome, turn_open))) = (status, applied) {
            crate::logging::info(
                "task_compaction_status_changed",
                serde_json::json!({
                    "task_id": self.task_id,
                    "session_id": self.session_id,
                    "compaction_id": compaction_id,
                    "status": format!("{status:?}"),
                    "outcome": outcome,
                    "turn_open": turn_open,
                }),
            );
        }
        Ok(())
    }
}
