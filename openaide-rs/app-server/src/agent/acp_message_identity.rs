use crate::protocol::model::AgentMessageRole;

/// Stable Chat identity for an ACP message within its Native Session.
pub(crate) fn stable_message_id(session_id: &str, source_message_id: &str) -> String {
    format!("acp:{session_id}:message:{source_message_id}")
}

/// User input reported by an Agent needs its own namespace within child history.
pub(crate) fn stable_user_message_id(session_id: &str, source_message_id: &str) -> String {
    format!("acp:{session_id}:user:{source_message_id}")
}

/// Agent text and private thought are separate Chat channels even when an Agent reuses one ACP ID.
pub(crate) fn stable_agent_message_id(
    session_id: &str,
    role: AgentMessageRole,
    source_message_id: &str,
) -> String {
    match role {
        // Preserve released visible-message identities; only private thought needs
        // a separate namespace to prevent cross-channel ACP ID collisions.
        AgentMessageRole::Agent => stable_message_id(session_id, source_message_id),
        AgentMessageRole::Thought => {
            format!("acp:{session_id}:thought:{source_message_id}")
        }
    }
}
