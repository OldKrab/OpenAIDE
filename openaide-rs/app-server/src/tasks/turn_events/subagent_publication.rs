//! Publishes committed Subagent changes to clients (ADR-0046). The catalog is
//! small and always republished; a history revision is published as its own
//! delta, and only while a client watches that history.

use openaide_app_server_protocol::events::TaskChatChange;

use crate::protocol::errors::RuntimeError;
use crate::snapshots::task_snapshot::{project_agent_plan, project_chat_item};
use crate::storage::subagents::{CommittedSubagentChange, SubagentHistoryCommit, SubagentRecord};

use super::TaskSessionEventSink;

impl TaskSessionEventSink {
    pub(super) fn publish_subagent_catalog(&self) -> Result<(), RuntimeError> {
        let catalog = crate::snapshots::task_snapshot::project_subagent_catalog(
            self.task_id.clone().into(),
            self.mutations.store().subagent_catalog(&self.task_id),
        )
        .map_err(|error| RuntimeError::Internal(error.message))?;
        self.mutations.publish_subagent_catalog(catalog);
        Ok(())
    }

    /// Publishes one committed history revision and returns its catalog entry.
    /// Each revision is published on its own, so a client applies them in order
    /// and re-reads the history when one is missing.
    pub(super) fn publish_subagent_history(&self, commit: SubagentHistoryCommit) -> SubagentRecord {
        let SubagentHistoryCommit { record, change } = commit;
        // A client that subscribes later reads a baseline that already holds this
        // revision, because it registers before reading.
        let watched = self.subscription_presence.as_ref().is_none_or(|presence| {
            presence.watches_subagent_history(&self.task_id, &record.subagent_id)
        });
        if !watched {
            return record;
        }
        let Some(change) = change else {
            // The watching client meets a revision gap and re-reads the history.
            crate::logging::warn(
                "subagent_history_change_unpublished",
                serde_json::json!({
                    "task_id": self.task_id,
                    "subagent_id": record.subagent_id,
                    "revision": record.history_revision,
                }),
            );
            return record;
        };
        let (chat, current_plan) = match change {
            CommittedSubagentChange::Append(message) => (
                vec![TaskChatChange::Append {
                    item: project_chat_item(&message),
                }],
                None,
            ),
            CommittedSubagentChange::Upsert(message) => (
                vec![TaskChatChange::Upsert {
                    item: project_chat_item(&message),
                }],
                None,
            ),
            CommittedSubagentChange::AppendText { message_id, text } => (
                vec![TaskChatChange::AppendText {
                    message_id: message_id.into(),
                    text,
                }],
                None,
            ),
            CommittedSubagentChange::Plan(plan) => (Vec::new(), Some(plan.map(project_agent_plan))),
        };
        self.mutations.publish_subagent_history_change(
            &self.task_id,
            &record.subagent_id,
            record.history_revision,
            chat,
            current_plan,
        );
        record
    }
}
