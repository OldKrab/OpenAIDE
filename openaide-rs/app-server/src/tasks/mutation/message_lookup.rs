use crate::protocol::model::NormalizedMessage;

use super::TaskMutationContext;

impl TaskMutationContext<'_> {
    /// The stored message with this identity, as visible to this commit.
    pub(crate) fn message_by_identity(&self, identity: &str) -> Option<&NormalizedMessage> {
        self.projection
            .messages
            .iter()
            .find(|stored| stored.chat.identity == identity)
            .map(|stored| &stored.chat.message)
    }
}

impl TaskMutationContext<'_> {
    /// Records how a background command ended on the Tool row that started it.
    /// Returns whether a row changed; a command with no visible row changes none.
    pub(crate) fn record_tool_background_outcome(
        &mut self,
        tool_call_id: &str,
        outcome: crate::protocol::model::BackgroundCommandOutcome,
    ) -> bool {
        use crate::protocol::model::ActivityStep;

        // Newest first: a reused Agent tool call id refers to its latest row.
        let Some(stored) = self
            .projection
            .messages
            .iter_mut()
            .rev()
            .find_map(|stored| {
                let NormalizedMessage::Activity { steps, .. } = &mut stored.chat.message else {
                    return None;
                };
                let slot = steps.iter_mut().find_map(|step| match step {
                    ActivityStep::Tool {
                        tool_call_id: Some(id),
                        background_outcome,
                        ..
                    } if id == tool_call_id => Some(background_outcome),
                    _ => None,
                })?;
                if *slot == Some(outcome) {
                    return None;
                }
                *slot = Some(outcome);
                Some(stored.clone())
            })
        else {
            return false;
        };
        crate::storage::message_store::advance_message_meta(self.projection, 0);
        self.chat_changes.push(super::CommittedChatChange::Upsert {
            item: crate::snapshots::task_snapshot::project_chat_item(&stored.chat),
        });
        true
    }
}
