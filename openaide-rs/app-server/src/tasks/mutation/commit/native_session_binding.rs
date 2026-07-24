use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ActivityStatus, NormalizedMessage, TaskStatus};
use crate::storage::records::{TaskLifecycle, TaskPreparationRecord, TaskRecord};
use crate::storage::task_journal::TaskProjection;

use super::super::{
    TaskCommitOptions, TaskCommitResult, TaskMutationContext, TaskMutationResult, TaskMutations,
};

pub(crate) fn replace_missing_session_for_initial_prompt(
    target: &TaskMutations,
    task_id: &str,
    turn_id: &str,
    expected_session_id: &str,
    options: TaskCommitOptions,
    mutation: impl FnOnce(&mut TaskMutationContext<'_>) -> Result<TaskMutationResult, RuntimeError>,
) -> Result<TaskCommitResult, RuntimeError> {
    super::commit_existing_task_with_session_policy(
        target,
        task_id,
        options,
        NativeSessionBindingPolicy::ReplaceMissingInitialPrompt {
            turn_id,
            expected_session_id,
        },
        mutation,
    )
}

pub(crate) fn replace_missing_session_for_prepared_task(
    target: &TaskMutations,
    task_id: &str,
    expected_session_id: &str,
    options: TaskCommitOptions,
    mutation: impl FnOnce(&mut TaskMutationContext<'_>) -> Result<TaskMutationResult, RuntimeError>,
) -> Result<TaskCommitResult, RuntimeError> {
    super::commit_existing_task_with_session_policy(
        target,
        task_id,
        options,
        NativeSessionBindingPolicy::ReplaceMissingPreparedTask {
            expected_session_id,
        },
        mutation,
    )
}

/// Controls the only two states where a confirmed-missing empty session may be rebound.
#[derive(Clone, Copy)]
pub(super) enum NativeSessionBindingPolicy<'a> {
    Preserve,
    ReplaceMissingPreparedTask {
        expected_session_id: &'a str,
    },
    ReplaceMissingInitialPrompt {
        turn_id: &'a str,
        expected_session_id: &'a str,
    },
}

impl NativeSessionBindingPolicy<'_> {
    pub(super) fn validate_replacement_boundary(
        self,
        projection: &TaskProjection,
    ) -> Result<(), RuntimeError> {
        let eligible = match self {
            Self::Preserve => return Ok(()),
            Self::ReplaceMissingPreparedTask {
                expected_session_id,
            } => {
                let task = &projection.task;
                task.agent_session_id.as_deref() == Some(expected_session_id)
                    && matches!(task.lifecycle, TaskLifecycle::Prepared { .. })
                    && matches!(task.preparation, TaskPreparationRecord::Preparing)
                    && projection.messages.is_empty()
            }
            Self::ReplaceMissingInitialPrompt {
                turn_id,
                expected_session_id,
            } => {
                let task = &projection.task;
                let initial_prompt_messages = matches!(
                    projection.messages.as_slice(),
                    [user, activity]
                        if matches!(user.chat.message, NormalizedMessage::User { .. })
                            && matches!(
                                &activity.chat.message,
                                NormalizedMessage::Activity {
                                    id,
                                    status: ActivityStatus::Running,
                                    ..
                                } if id == &format!("turn:{turn_id}")
                            )
                );
                task.agent_session_id.as_deref() == Some(expected_session_id)
                    && task.active_turn_id.as_deref() == Some(turn_id)
                    && task.status == TaskStatus::Starting
                    && initial_prompt_messages
            }
        };
        if !eligible {
            return Err(RuntimeError::NotReady(
                "Task is no longer eligible for missing Native Session recovery".to_string(),
            ));
        }
        Ok(())
    }

    fn allows_replacement(self, replacement: Option<&str>) -> bool {
        !matches!(self, Self::Preserve) && replacement.is_some()
    }
}

pub(super) struct VersionFields {
    task_version: u64,
    message_history_version: u64,
    revision: u64,
    bound_native_session_id: Option<String>,
}

impl VersionFields {
    pub(super) fn from_task(task: &TaskRecord) -> Self {
        Self {
            task_version: task.task_version,
            message_history_version: task.message_history_version,
            revision: task.revision,
            bound_native_session_id: task.agent_session_id.clone(),
        }
    }
}

pub(super) fn validate_task_invariants(
    requested_task_id: &str,
    original: &VersionFields,
    task: &TaskRecord,
    session_policy: NativeSessionBindingPolicy<'_>,
) -> Result<(), RuntimeError> {
    if task.task_id != requested_task_id {
        return Err(RuntimeError::Internal(
            "task mutation changed task identity".to_string(),
        ));
    }
    if task.task_version != original.task_version
        || task.message_history_version != original.message_history_version
        || task.revision != original.revision
    {
        return Err(RuntimeError::Internal(
            "task mutation changed commit-managed version fields".to_string(),
        ));
    }
    if original.bound_native_session_id.is_some()
        && task.agent_session_id != original.bound_native_session_id
        && !session_policy.allows_replacement(task.agent_session_id.as_deref())
    {
        return Err(RuntimeError::Internal(
            "task mutation changed bound Native Session identity".to_string(),
        ));
    }
    Ok(())
}
