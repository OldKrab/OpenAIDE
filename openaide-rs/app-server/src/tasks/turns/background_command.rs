use crate::protocol::errors::RuntimeError;

use super::TurnRunner;

impl TurnRunner {
    /// Asks the Agent to stop one background command of an active turn and
    /// waits for its answer. The turn keeps running: the command leaves the
    /// Task through the Agent's own ordered report.
    pub(crate) fn stop_background_command(
        &self,
        turn_id: &str,
        command_id: &str,
    ) -> Result<(), RuntimeError> {
        let session = self
            .active_turns
            .turns
            .lock()
            .expect("active turn registry poisoned")
            .get(turn_id)
            .map(|active| active.session.clone())
            .ok_or_else(|| RuntimeError::Conflict("Task turn is not active".to_string()))?;
        self.agent.stop_background_command(&session, command_id)
    }
}
