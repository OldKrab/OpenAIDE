use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, NormalizedMessage, ToolPermissionOutcome,
};
use crate::storage::records::StoredMessage;

use super::Store;

impl Store {
    /// Appends or replaces one App Server-owned authorization decision on its exact tool row.
    pub fn record_tool_permission_outcome(
        &self,
        task_id: &str,
        activity_identity: &str,
        tool_call_id: &str,
        outcome: ToolPermissionOutcome,
    ) -> Result<Vec<StoredMessage>, RuntimeError> {
        let mut projection = self.task_journal().load(task_id)?;
        let Some(stored) = projection
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == activity_identity)
        else {
            return Ok(Vec::new());
        };
        let NormalizedMessage::Activity { steps, .. } = &mut stored.chat.message else {
            return Ok(Vec::new());
        };
        let Some(permission_outcomes) = steps.iter_mut().find_map(|step| match step {
            ActivityStep::Tool {
                tool_call_id: Some(id),
                permission_outcomes,
                ..
            } if id == tool_call_id => Some(permission_outcomes),
            _ => None,
        }) else {
            return Ok(Vec::new());
        };
        if let Some(existing) = permission_outcomes
            .iter_mut()
            .find(|existing| existing.request_id == outcome.request_id)
        {
            *existing = outcome;
        } else {
            permission_outcomes.push(outcome);
        }
        let updated = stored.clone();
        super::advance_message_meta(&mut projection, 0);
        self.commit_task_projection(projection)?;
        Ok(vec![updated])
    }

    pub fn finish_running_activities(
        &self,
        task_id: &str,
        status: ActivityStatus,
    ) -> Result<Vec<StoredMessage>, RuntimeError> {
        let mut projection = self.task_journal().load(task_id)?;
        let mut changed = Vec::new();
        for stored in projection.messages.iter_mut().rev() {
            if finish_running_activity(&mut stored.chat.message, status) {
                changed.push(stored.clone());
            }
        }
        if !changed.is_empty() {
            super::advance_message_meta(&mut projection, 0);
            self.commit_task_projection(projection)?;
        }
        Ok(changed)
    }

    pub fn finish_running_activity_by_identity(
        &self,
        task_id: &str,
        identity: &str,
        status: ActivityStatus,
    ) -> Result<Vec<StoredMessage>, RuntimeError> {
        let mut projection = self.task_journal().load(task_id)?;
        let Some(stored) = projection
            .messages
            .iter_mut()
            .find(|stored| stored.chat.identity == identity)
        else {
            return Ok(Vec::new());
        };
        let changed = finish_running_activity(&mut stored.chat.message, status);
        let updated = changed.then(|| stored.clone()).into_iter().collect();
        if changed {
            super::advance_message_meta(&mut projection, 0);
            self.commit_task_projection(projection)?;
        }
        Ok(updated)
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
