use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStatus, ActivityStep, NormalizedMessage, PermissionDecision, PermissionOptionKind,
    PermissionState, QuestionAction, QuestionState,
};

use super::Store;

impl Store {
    pub fn finish_streaming_messages(&self, task_id: &str) -> Result<bool, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let mut changed = false;
        for stored in &mut messages {
            match &mut stored.chat.message {
                NormalizedMessage::AgentText { streaming, .. }
                | NormalizedMessage::Thought { streaming, .. }
                    if *streaming =>
                {
                    *streaming = false;
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            self.write_messages(task_id, &messages)?;
            self.write_meta(task_id, &messages)?;
        }
        Ok(changed)
    }

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

    pub fn resolve_permission(
        &self,
        task_id: &str,
        request_id: &str,
        option_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let mut found = false;
        for stored in &mut messages {
            if let NormalizedMessage::Permission {
                request_id: stored_request_id,
                state,
                options,
                selected_option,
                decision: stored_decision,
                ..
            } = &mut stored.chat.message
            {
                if stored_request_id != request_id {
                    continue;
                }
                found = true;
                if *state == PermissionState::Resolved {
                    return Err(RuntimeError::InvalidParams(
                        "permission already resolved".to_string(),
                    ));
                }
                let option = options
                    .iter()
                    .find(|option| option.id == option_id)
                    .ok_or_else(|| RuntimeError::InvalidParams("option_id".to_string()))?;
                validate_permission_decision(option.kind, decision)?;
                *state = PermissionState::Resolved;
                *selected_option = Some(option_id.to_string());
                *stored_decision = Some(decision);
                break;
            }
        }
        if !found {
            return Err(RuntimeError::InvalidParams("request_id".to_string()));
        }
        self.write_messages(task_id, &messages)?;
        self.write_meta(task_id, &messages)
    }

    pub fn cancel_pending_permissions(&self, task_id: &str) -> Result<bool, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let mut changed = false;
        for stored in &mut messages {
            if let NormalizedMessage::Permission {
                state,
                selected_option,
                decision,
                ..
            } = &mut stored.chat.message
            {
                if *state != PermissionState::Resolved && *state != PermissionState::Cancelled {
                    *state = PermissionState::Cancelled;
                    *selected_option = None;
                    *decision = None;
                    changed = true;
                }
            }
        }
        if changed {
            self.write_messages(task_id, &messages)?;
            self.write_meta(task_id, &messages)?;
        }
        Ok(changed)
    }

    pub fn resolve_question(
        &self,
        task_id: &str,
        request_id: &str,
        response: &openaide_app_server_protocol::server_requests::QuestionRequestResponse,
    ) -> Result<bool, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let mut found = false;
        for stored in &mut messages {
            let NormalizedMessage::Question { request_id: stored_id, state, action, content, .. } =
                &mut stored.chat.message
            else { continue; };
            if stored_id != request_id { continue; }
            if *state != QuestionState::Pending {
                return Err(RuntimeError::InvalidParams("question already resolved".to_string()));
            }
            found = true;
            match response {
                openaide_app_server_protocol::server_requests::QuestionRequestResponse::Submit { content: submitted } => {
                    *state = QuestionState::Resolved;
                    *action = Some(QuestionAction::Submit);
                    *content = Some(submitted.clone());
                }
                openaide_app_server_protocol::server_requests::QuestionRequestResponse::Cancel => {
                    *state = QuestionState::Cancelled;
                    *action = Some(QuestionAction::Cancel);
                    *content = None;
                }
            }
            break;
        }
        if !found { return Err(RuntimeError::InvalidParams("request_id".to_string())); }
        let has_pending = messages.iter().any(|stored| matches!(stored.chat.message,
            NormalizedMessage::Question { state: QuestionState::Pending, .. }));
        self.write_messages(task_id, &messages)?;
        self.write_meta(task_id, &messages)?;
        Ok(has_pending)
    }

    pub fn cancel_pending_questions(&self, task_id: &str) -> Result<bool, RuntimeError> {
        let mut messages = self.read_messages(task_id)?;
        let mut changed = false;
        for stored in &mut messages {
            if let NormalizedMessage::Question {
                state,
                action,
                content,
                ..
            } = &mut stored.chat.message
            {
                if *state == QuestionState::Pending {
                    *state = QuestionState::Cancelled;
                    *action = Some(QuestionAction::Cancel);
                    *content = None;
                    changed = true;
                }
            }
        }
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

fn validate_permission_decision(
    option_kind: Option<PermissionOptionKind>,
    decision: PermissionDecision,
) -> Result<(), RuntimeError> {
    match (option_kind, decision) {
        (Some(PermissionOptionKind::Allow), PermissionDecision::Approved)
        | (Some(PermissionOptionKind::Deny), PermissionDecision::Denied)
        | (Some(PermissionOptionKind::Other), _) => Ok(()),
        (None, _) => Ok(()),
        _ => Err(RuntimeError::InvalidParams(
            "decision does not match option kind".to_string(),
        )),
    }
}
