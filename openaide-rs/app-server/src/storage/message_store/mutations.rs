use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, ActivityStep, NormalizedMessage};

use super::Store;

impl Store {
    pub fn finish_running_activities(
        &self,
        task_id: &str,
        status: ActivityStatus,
    ) -> Result<bool, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let mut changed = false;
        for stored in messages.iter_mut().rev() {
            changed |= finish_running_activity(&mut stored.chat.message, status);
        }
        if changed {
            self.write_messages(task_id, &messages)?;
            self.write_meta(task_id, &messages)?;
        }
        Ok(changed)
    }

    pub fn finish_running_activity_by_identity(
        &self,
        task_id: &str,
        identity: &str,
        status: ActivityStatus,
    ) -> Result<bool, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let Some(stored) = messages
            .iter_mut()
            .find(|stored| stored.chat.identity == identity)
        else {
            return Ok(false);
        };
        let changed = finish_running_activity(&mut stored.chat.message, status);
        if changed {
            self.write_messages(task_id, &messages)?;
            self.write_meta(task_id, &messages)?;
        }
        Ok(changed)
    }
}

fn finish_running_activity(message: &mut NormalizedMessage, status: ActivityStatus) -> bool {
    let NormalizedMessage::Activity {
        status: activity_status,
        steps,
        ..
    } = message
    else {
        return false;
    };
    if *activity_status != ActivityStatus::Running {
        return false;
    }
    *activity_status = status;
    for step in steps {
        match step {
            ActivityStep::Tool {
                status: step_status,
                ..
            }
            | ActivityStep::Command {
                status: step_status,
                ..
            } if *step_status == ActivityStatus::Running => *step_status = status,
            _ => {}
        }
    }
    true
}
