use super::*;
use crate::agent::AgentPromptOutcome;

impl TurnRunner {
    /// Settlement and successor admission share the same gate as Send and Delete.
    pub(super) fn settle_turn_and_advance_queue(
        &self,
        task_id: &str,
        turn_id: &str,
        result: Result<AgentPromptOutcome, RuntimeError>,
        next_session: AgentSession,
        next_session_sink: Arc<TaskSessionEventSink>,
    ) -> Result<bool, RuntimeError> {
        self.turn_acceptance.serialize(task_id, || {
            #[cfg(test)]
            self.pause_before_settlement_for_test();
            if self.turn_acceptance.session_deletion_unresolved(
                &crate::native_sessions::catalog::NativeSessionRef::new(
                    &next_session.agent_id,
                    &next_session.session_id,
                ),
            ) {
                // Settle the existing turn but retain queued work while Agent deletion
                // is unresolved. A terminal prompt response does not resolve Delete.
                return self.transitions().finish_turn(task_id, turn_id, result);
            }
            let next_turn_id = format!("turn_{}", uuid::Uuid::new_v4());
            let next_message_id = format!("message_{}", uuid::Uuid::new_v4());
            if !self
                .turn_acceptance
                .own_pending_turn(task_id, &next_turn_id)
            {
                return self.transitions().finish_turn(task_id, turn_id, result);
            }
            let accepted = self.transitions().finish_turn_and_accept_queue(
                task_id,
                turn_id,
                result,
                &next_turn_id,
                &next_message_id,
            );
            match accepted {
                Ok(Some(queued)) => {
                    self.spawn_agent_turn(
                        task_id.to_string(),
                        queued.text,
                        queued.attachments,
                        queued.turn_id.clone(),
                        next_session,
                        next_session_sink,
                    );
                    self.turn_acceptance
                        .retire_pending_turn(task_id, &queued.turn_id);
                    Ok(true)
                }
                Ok(None) => {
                    self.turn_acceptance
                        .retire_pending_turn(task_id, &next_turn_id);
                    Ok(true)
                }
                Err(error) => {
                    self.turn_acceptance
                        .retire_pending_turn(task_id, &next_turn_id);
                    Err(error)
                }
            }
        })
    }
}
