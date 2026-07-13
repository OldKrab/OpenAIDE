use std::time::Duration;

use agent_client_protocol::{Agent, SessionMessage};

use crate::agent::acp_errors::acp_error;
use crate::protocol::errors::RuntimeError;

/// Takes every session message queued before a held ACP response callback.
///
/// The zero-duration read is a non-blocking queue probe. The caller must hold the
/// response callback so later wire messages cannot enter the queue during this drain.
pub(super) async fn take_preceding_session_updates(
    active_session: &mut agent_client_protocol::ActiveSession<'static, Agent>,
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
