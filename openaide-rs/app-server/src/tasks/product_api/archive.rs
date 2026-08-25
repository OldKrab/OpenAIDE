use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{AgentId, ClientInstanceId, ProjectId, TaskId};
use openaide_app_server_protocol::snapshot::NativeSessionReference;
use openaide_app_server_protocol::task::{
    TaskArchiveOlderCutoff, TaskArchiveOlderParams, TaskArchiveOlderProtectedNativeSession,
    TaskArchiveOlderProtectedReason, TaskArchiveOlderProtectedTask, TaskArchiveOlderResult,
    TaskArchiveParams, TaskLifecycleChanged, TaskRestoreParams,
};

use crate::protocol::model::TaskStatus;
use crate::snapshots::project_task_summary;
use crate::storage::records::TaskLifecycle;
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::time::now_string;

use super::{conflict_error, protocol_error_from_runtime, TaskProductApi};

impl TaskProductApi {
    pub(super) fn archive_older_tasks(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskArchiveOlderParams,
    ) -> Result<TaskArchiveOlderResult, ProtocolError> {
        let _native_mutation = self.native_adoption.lock().map_err(|_| {
            protocol_error_from_runtime(crate::protocol::errors::RuntimeError::Internal(
                "Native Session mutation lock poisoned".to_string(),
            ))
        })?;
        let preview = params.preview;
        let cutoff = params.cutoff;
        let (project_id, cutoff_activity, cutoff_task_id, cutoff_native_ref) = match &cutoff {
            TaskArchiveOlderCutoff::Task { task_id } => {
                let task = self.read_task_for_client(task_id.as_str(), client_instance_id)?;
                if !matches!(task.lifecycle, TaskLifecycle::Open) {
                    return Err(conflict_error("The cutoff Task must be open"));
                }
                let activity =
                    crate::time::activity_millis(&task.last_activity).ok_or_else(|| {
                        conflict_error("The cutoff Task has invalid activity metadata")
                    })?;
                let project_id = crate::projects::ProjectIdentity::from_workspace_root(
                    task.project_root.as_deref().unwrap_or(&task.workspace_root),
                )
                .project_id
                .as_str()
                .to_string();
                (project_id, activity, Some(task.task_id), None)
            }
            TaskArchiveOlderCutoff::NativeSession {
                agent_id,
                native_session_id,
            } => {
                let reference = crate::native_sessions::catalog::NativeSessionRef::new(
                    agent_id.as_str(),
                    native_session_id,
                );
                let entry = self.native_catalog.entry(&reference).ok_or_else(|| {
                    openaide_app_server_protocol::errors::ProtocolError {
                        code: openaide_app_server_protocol::errors::ProtocolErrorCode::NotFound,
                        message: "The cutoff Native Session was not found".to_string(),
                        recoverable: false,
                        target: None,
                    }
                })?;
                if self.native_catalog.is_archived(&reference) {
                    return Err(conflict_error("The cutoff Native Session must be open"));
                }
                let activity = entry
                    .observation
                    .last_activity
                    .as_deref()
                    .and_then(crate::time::activity_millis)
                    .ok_or_else(|| {
                        conflict_error("The cutoff Native Session has invalid activity metadata")
                    })?;
                (entry.project_id, activity, None, Some(reference))
            }
        };
        crate::logging::info(
            "task_archive_older_started",
            serde_json::json!({
                "project_id": project_id.as_str(),
                "preview": preview,
            }),
        );
        let all_tasks = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?;
        let owned_native_sessions = all_tasks
            .iter()
            .filter(|task| !task.tombstoned)
            .filter_map(|task| {
                task.agent_session_id
                    .as_ref()
                    .map(|session_id| (task.agent_id.clone(), session_id.clone()))
            })
            .collect::<std::collections::HashSet<_>>();
        let mut candidates = all_tasks
            .into_iter()
            .filter(|task| cutoff_task_id.as_deref() != Some(task.task_id.as_str()))
            .filter(|task| matches!(task.lifecycle, TaskLifecycle::Open))
            .filter(|task| {
                crate::projects::ProjectIdentity::from_workspace_root(
                    task.project_root.as_deref().unwrap_or(&task.workspace_root),
                )
                .project_id
                .as_str()
                    == project_id
            })
            .filter_map(|task| {
                crate::time::activity_millis(&task.last_activity)
                    .filter(|activity| *activity < cutoff_activity)
                    .map(|activity| (activity, task))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(left_activity, left), (right_activity, right)| {
            left_activity
                .cmp(right_activity)
                .then_with(|| left.task_id.cmp(&right.task_id))
        });

        let mut eligible_task_ids = Vec::new();
        let mut protected = Vec::new();
        for (_, task) in candidates {
            let reason = if task.pinned {
                Some(TaskArchiveOlderProtectedReason::Pinned)
            } else if task.active_turn_id.is_some()
                || matches!(
                    task.status,
                    TaskStatus::Starting
                        | TaskStatus::Active
                        | TaskStatus::Stopping
                        | TaskStatus::Waiting
                )
            {
                Some(TaskArchiveOlderProtectedReason::Active)
            } else if !task.message_queue.items.is_empty() {
                Some(TaskArchiveOlderProtectedReason::Queued)
            } else if self
                .server_requests
                .has_pending_for_task(&TaskId::from(task.task_id.clone()))
            {
                Some(TaskArchiveOlderProtectedReason::PendingRequest)
            } else if self
                .task_subscription_presence
                .has_subscribers(&task.task_id)
            {
                Some(TaskArchiveOlderProtectedReason::OpenElsewhere)
            } else {
                None
            };
            if let Some(reason) = reason {
                protected.push(TaskArchiveOlderProtectedTask {
                    task_id: TaskId::from(task.task_id),
                    reason,
                });
            } else {
                eligible_task_ids.push(TaskId::from(task.task_id));
            }
        }

        let mut native_candidates = self
            .native_catalog
            .entries()
            .into_iter()
            .filter(|entry| entry.project_id == project_id)
            .filter(|entry| {
                !self
                    .native_catalog
                    .is_archived(&entry.observation.reference)
            })
            .filter(|entry| {
                cutoff_native_ref.as_ref() != Some(&entry.observation.reference)
                    && !owned_native_sessions.contains(&(
                        entry.observation.reference.agent_id.clone(),
                        entry.observation.reference.session_id.clone(),
                    ))
            })
            .filter_map(|entry| {
                entry
                    .observation
                    .last_activity
                    .as_deref()
                    .and_then(crate::time::activity_millis)
                    .filter(|activity| *activity < cutoff_activity)
                    .map(|activity| (activity, entry))
            })
            .collect::<Vec<_>>();
        native_candidates.sort_by(|(left_activity, left), (right_activity, right)| {
            left_activity
                .cmp(right_activity)
                .then_with(|| {
                    left.observation
                        .reference
                        .agent_id
                        .cmp(&right.observation.reference.agent_id)
                })
                .then_with(|| {
                    left.observation
                        .reference
                        .session_id
                        .cmp(&right.observation.reference.session_id)
                })
        });
        let mut eligible_native_sessions = Vec::new();
        let mut protected_native_sessions = Vec::new();
        let mut eligible_native_refs = Vec::new();
        for (_, entry) in native_candidates {
            let reference = NativeSessionReference {
                agent_id: AgentId::from(entry.observation.reference.agent_id.clone()),
                session_id: entry.observation.reference.session_id.clone(),
            };
            if entry.pinned {
                protected_native_sessions.push(TaskArchiveOlderProtectedNativeSession {
                    reference,
                    reason: TaskArchiveOlderProtectedReason::Pinned,
                });
            } else {
                eligible_native_sessions.push(reference);
                eligible_native_refs.push(entry.observation.reference);
            }
        }

        let mut archived_task_ids = Vec::new();
        let mut archived_native_sessions = Vec::new();
        if !preview {
            for task_id in eligible_task_ids.clone() {
                match self.archive_task(
                    client_instance_id,
                    TaskArchiveParams {
                        task_id: task_id.clone(),
                    },
                ) {
                    Ok(_) => archived_task_ids.push(task_id),
                    Err(error)
                        if error.code
                            == openaide_app_server_protocol::errors::ProtocolErrorCode::Conflict =>
                    {
                        protected.push(TaskArchiveOlderProtectedTask {
                            task_id,
                            reason: TaskArchiveOlderProtectedReason::Changed,
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
            let archived_native_results = self
                .native_catalog
                .archive_many(&eligible_native_refs)
                .map_err(protocol_error_from_runtime)?;
            for (archived, protocol_reference) in archived_native_results
                .into_iter()
                .zip(eligible_native_sessions.iter())
            {
                if archived {
                    archived_native_sessions.push(protocol_reference.clone());
                }
            }
        }
        let result = TaskArchiveOlderResult {
            project_id: ProjectId::from(project_id),
            cutoff,
            eligible_task_ids,
            eligible_native_sessions,
            protected,
            protected_native_sessions,
            archived_task_ids,
            archived_native_sessions,
        };
        crate::logging::info(
            "task_archive_older_completed",
            serde_json::json!({
                "project_id": result.project_id.as_str(),
                "preview": preview,
                "eligible_count": result.eligible_task_ids.len() + result.eligible_native_sessions.len(),
                "archived_count": result.archived_task_ids.len() + result.archived_native_sessions.len(),
                "protected_count": result.protected.len() + result.protected_native_sessions.len(),
            }),
        );
        Ok(result)
    }

    pub(super) fn archive_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskArchiveParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        self.session_operations.serialize(&task_id, || {
            self.archive_task_serialized(client_instance_id, params)
        })
    }

    fn archive_task_serialized(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskArchiveParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        let task_id = params.task_id.clone();
        let task = self.read_task_for_client(task_id.as_str(), client_instance_id)?;
        if matches!(task.lifecycle, TaskLifecycle::Prepared { .. }) {
            return Err(conflict_error(
                "Prepared Tasks cannot be archived; discard it explicitly instead",
            ));
        }
        if matches!(task.lifecycle, TaskLifecycle::Archived) {
            return Ok(lifecycle_change(&task, TaskLifecycle::Archived));
        }
        if task.active_turn_id.is_some()
            || matches!(
                task.status,
                TaskStatus::Starting | TaskStatus::Active | TaskStatus::Stopping
            )
            || self.server_requests.has_pending_for_task(&task_id)
        {
            return Err(conflict_error(
                "A Task must be idle before it can be archived",
            ));
        }

        self.transition_task_lifecycle(task, TaskLifecycle::Archived)
    }

    pub(super) fn restore_task(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskRestoreParams,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        let task = self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        if matches!(task.lifecycle, TaskLifecycle::Prepared { .. }) {
            return Err(conflict_error("Prepared Tasks cannot be restored"));
        }
        if matches!(task.lifecycle, TaskLifecycle::Open) {
            return Ok(lifecycle_change(&task, TaskLifecycle::Open));
        }
        self.transition_task_lifecycle(task, TaskLifecycle::Open)
    }

    fn transition_task_lifecycle(
        &self,
        task: crate::storage::records::TaskRecord,
        next_lifecycle: TaskLifecycle,
    ) -> Result<TaskLifecycleChanged, ProtocolError> {
        let task_id = task.task_id.clone();
        let previous_lifecycle = task.lifecycle.clone();
        let now = now_string();
        let result = self
            .mutations
            .commit_existing_task(task_id.as_str(), TaskCommitOptions::metadata(), |ctx| {
                let task = ctx.task_mut();
                task.lifecycle = next_lifecycle.clone();
                if matches!(next_lifecycle, TaskLifecycle::Archived) {
                    task.pinned = false;
                    task.clear_process_local_agent_state();
                }
                task.updated_at = now;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let TaskCommitOutcome::Committed(facts) = result.outcome else {
            return Err(conflict_error("Task lifecycle did not change"));
        };
        Ok(lifecycle_change(&facts.committed_task, previous_lifecycle))
    }
}

fn lifecycle_change(
    task: &crate::storage::records::TaskRecord,
    previous_lifecycle: TaskLifecycle,
) -> TaskLifecycleChanged {
    TaskLifecycleChanged {
        previous_lifecycle: crate::snapshots::project_task_lifecycle(&previous_lifecycle),
        task: project_task_summary(task.clone()),
    }
}
