use crate::agent::acp_compaction_projection::apply_compaction_change;
use crate::agent::acp_message_identity::stable_compaction_id;
use crate::agent::events::AgentCompactionChange;
use crate::protocol::errors::RuntimeError;
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
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str()) {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let message =
                    apply_compaction_change(&id, ctx.message_by_identity(&id), change, now);
                ctx.upsert_message_with_details(message)?;
                ctx.task_mut().updated_at = now.to_string();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        Ok(())
    }
}
