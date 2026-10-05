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
