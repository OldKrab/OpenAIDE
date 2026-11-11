use std::time::Duration;

use agent_client_protocol::v1::SessionMessage;
use agent_client_protocol::{Agent, Dispatch};

use crate::agent::acp_errors::acp_error;
use crate::protocol::errors::RuntimeError;

/// Takes every session message queued before a held ACP response callback.
///
/// The zero-duration read is a non-blocking queue probe. The caller must hold the
/// response callback so later wire messages cannot enter the queue during this drain.
pub(super) async fn take_preceding_session_updates(
    active_session: &mut agent_client_protocol::v1::ActiveSession<'static, Agent>,
) -> Result<Vec<SessionMessage>, RuntimeError> {
    let mut updates = Vec::new();
    loop {
        let Ok(update) = tokio::time::timeout(Duration::ZERO, active_session.read_update()).await
        else {
            return Ok(updates);
        };
        updates.push(update.map_err(acp_error)?);
    }
}

/// Settles a session-stream message no OpenAIDE handler matched. A request is answered with
/// method not found, so the Agent never waits on a host method OpenAIDE lacks, and logged
/// with its method name only; a notification or response is ignored.
pub(super) fn answer_unmatched(
    agent_id: &str,
    dispatch: Dispatch,
) -> Result<(), agent_client_protocol::Error> {
    let Dispatch::Request(request, responder) = dispatch else {
        return Ok(());
    };
    let method = request.method();
    let method = if method.len() <= 64
        && method
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_/.-".contains(&byte))
    {
        method
    } else {
        "unreadable"
    };
    crate::logging::warn(
        "acp_session_request_unhandled",
        serde_json::json!({
            "agent_id": agent_id,
            "method": method,
            "error_code": agent_client_protocol::ErrorCode::METHOD_NOT_FOUND.value(),
        }),
    );
    responder.respond_with_error(agent_client_protocol::Error::method_not_found())
}
