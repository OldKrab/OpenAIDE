use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::agent::AgentPrompt;
use crate::protocol::errors::RuntimeError;
use crate::tasks::turn_events::TaskEventSink;

use super::{ActiveTurn, TurnRunner};

#[derive(Default)]
pub(super) struct SteeringState {
    pub(super) pending: usize,
    pub(super) terminalizing: bool,
}

impl TurnRunner {
    pub(crate) fn reserve_steering(
        &self,
        task_id: &str,
        turn_id: &str,
    ) -> Result<SteeringReservation, RuntimeError> {
        let active = self
            .active_turns
            .lock()
            .expect("active turn registry poisoned")
            .get(turn_id)
            .cloned()
            .ok_or_else(|| RuntimeError::NotReady("active turn is not live".to_string()))?;
        if active.task_id != task_id {
            return Err(RuntimeError::InvalidParams(
                "active turn belongs to another task".to_string(),
            ));
        }
        if active.cancellation.is_cancelled() {
            return Err(RuntimeError::NotReady(
                "active turn is stopping".to_string(),
            ));
        }
        let mut steering = active
            .steering
            .lock()
            .expect("active turn steering state poisoned");
        if steering.terminalizing {
            return Err(RuntimeError::NotReady(
                "active turn is finishing".to_string(),
            ));
        }
        steering.pending += 1;
        drop(steering);
        Ok(SteeringReservation {
            runner: self.clone(),
            active: Some(active),
            task_id: task_id.to_string(),
            turn_id: turn_id.to_string(),
        })
    }

    fn spawn_reserved_steering(
        &self,
        active: ActiveTurn,
        task_id: String,
        turn_id: String,
        prompt_text: String,
        prompt_attachments: Vec<crate::protocol::model::Attachment>,
    ) {
        let runner = self.clone();
        thread::spawn(move || {
            if active.cancellation.is_cancelled() || !runner.turn_is_active(&task_id, &turn_id) {
                runner.release_steering(&active);
                return;
            }
            let sink = Arc::new(TaskEventSink::new(
                runner.mutations.clone(),
                task_id.clone(),
                turn_id.clone(),
                runner.server_requests.clone(),
                active.cancellation.clone(),
            ));
            let result = runner.agent.prompt(
                AgentPrompt {
                    task_id: task_id.clone(),
                    session_id: active.session_id.clone(),
                    text: prompt_text,
                    attachments: prompt_attachments,
                    cancellation: active.cancellation.clone(),
                },
                sink.clone(),
            );
            let _ = result.and(sink.finish());
            runner.release_steering(&active);
        });
    }

    pub(super) fn wait_until_steering_finishes(
        &self,
        task_id: &str,
        turn_id: &str,
        active: &ActiveTurn,
    ) -> bool {
        {
            let mut steering = active
                .steering
                .lock()
                .expect("active turn steering state poisoned");
            steering.terminalizing = true;
        }
        loop {
            if active.cancellation.is_cancelled() || !self.turn_is_active(task_id, turn_id) {
                return false;
            }
            let steering = active
                .steering
                .lock()
                .expect("active turn steering state poisoned");
            let pending = steering.pending;
            drop(steering);
            if pending == 0 {
                return true;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn release_steering(&self, active: &ActiveTurn) {
        let mut steering = active
            .steering
            .lock()
            .expect("active turn steering state poisoned");
        debug_assert!(steering.pending > 0);
        steering.pending = steering.pending.saturating_sub(1);
    }
}

pub(crate) struct SteeringReservation {
    runner: TurnRunner,
    active: Option<ActiveTurn>,
    task_id: String,
    turn_id: String,
}

impl SteeringReservation {
    pub(crate) fn dispatch(
        mut self,
        prompt_text: String,
        prompt_attachments: Vec<crate::protocol::model::Attachment>,
    ) -> Result<(), RuntimeError> {
        let active = self.active.as_ref().expect("steering reservation consumed");
        if active.cancellation.is_cancelled()
            || !self
                .runner
                .active_turn_is_live(&self.task_id, &self.turn_id)
        {
            return Err(RuntimeError::NotReady(
                "active turn is not live".to_string(),
            ));
        }
        let active = self.active.take().expect("steering reservation consumed");
        self.runner.spawn_reserved_steering(
            active,
            self.task_id.clone(),
            self.turn_id.clone(),
            prompt_text,
            prompt_attachments,
        );
        Ok(())
    }
}

impl Drop for SteeringReservation {
    fn drop(&mut self) {
        if let Some(active) = self.active.take() {
            self.runner.release_steering(&active);
        }
    }
}
