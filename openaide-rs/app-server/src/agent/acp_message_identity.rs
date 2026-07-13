/// Stable Chat identity for an ACP message within its Native Session.
pub(crate) fn stable_message_id(session_id: &str, source_message_id: &str) -> String {
    format!("acp:{session_id}:message:{source_message_id}")
}
