use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::storage::Store;
use crate::tasks::mutation::create_validation::TaskCreationValidationContext;
use crate::tasks::snapshot::build_snapshot;

use super::{
    TaskCommitFacts, TaskCommitOptions, TaskCommitOutcome, TaskCommitRejection, TaskCommitResult,
    TaskMutationContext, TaskMutationResult, TaskMutations,
};

pub(super) fn commit_existing_task(
    target: &TaskMutations,
    task_id: &str,
    options: TaskCommitOptions,
    mutation: impl FnOnce(&mut TaskMutationContext<'_>) -> Result<TaskMutationResult, RuntimeError>,
) -> Result<TaskCommitResult, RuntimeError> {
    let _guard = target.lock();
    let mut task = target.store.read_task(task_id)?;
    let original_active_turn = task.active_turn_id.clone();
    let original_version_fields = VersionFields::from_task(&task);
    let message_backup = target.store.backup_message_files(task_id)?;
    let send_receipts_backup = target.store.backup_send_receipts(task_id)?;
    let rollback = || {
        let message_result = target.store.restore_message_files(task_id, &message_backup);
        let receipt_result = target
            .store
            .restore_send_receipts(task_id, send_receipts_backup.as_deref());
        message_result.and(receipt_result)
    };
    let mut ctx = TaskMutationContext {
        store: &target.store,
        task: &mut task,
        committed_delta: None,
    };
    let mutation_result = match mutation(&mut ctx) {
        Ok(result) => result,
        Err(error) => {
            rollback()?;
            return Err(error);
        }
    };
    let committed_delta = ctx.committed_delta.take();
    drop(ctx);
    let outcome = match mutation_result {
        TaskMutationResult::Changed => {
            if original_active_turn.is_some() && task.active_turn_id.is_none() {
                if let Err(error) = target.store.finish_streaming_messages(task_id) {
                    rollback()?;
                    return Err(error);
                }
            }
            if let Err(error) = validate_task_invariants(task_id, &original_version_fields, &task) {
                rollback()?;
                return Err(error);
            }
            let facts = match persist_changed_task(target, &mut task, options, committed_delta) {
                Ok(facts) => facts,
                Err(error) => {
                    rollback()?;
                    return Err(error);
                }
            };
            notify_task_updated(target, &facts);
            TaskCommitOutcome::Committed(facts)
        }
        TaskMutationResult::Unchanged => {
            rollback()?;
            TaskCommitOutcome::Rejected(TaskCommitRejection::NoChange)
        }
        TaskMutationResult::Rejected => {
            rollback()?;
            TaskCommitOutcome::Rejected(TaskCommitRejection::NoChange)
        }
    };
    let snapshot = match options.response_snapshot_tail_limit {
        Some(limit) => Some(build_snapshot(&target.store, task_id, limit)?),
        None => None,
    };
    Ok(TaskCommitResult {
        outcome,
        response_snapshot: snapshot,
    })
}

pub(super) fn create_task_with_validation_and_writer(
    target: &TaskMutations,
    mut task: TaskRecord,
    initial_messages: Vec<NormalizedMessage>,
    options: TaskCommitOptions,
    validate: impl FnOnce(&TaskCreationValidationContext<'_>) -> Result<(), RuntimeError>,
    write_task: impl FnOnce(&Store, &TaskRecord) -> Result<(), RuntimeError>,
) -> Result<TaskCommitResult, RuntimeError> {
    let _guard = target.lock();
    let task_id = task.task_id.clone();
    match target.store.read_task(&task_id) {
        Ok(_) => return Err(RuntimeError::InvalidParams("task_id".to_string())),
        Err(RuntimeError::TaskNotFound(_)) => {}
        Err(error) => return Err(error),
    }
    validate(&TaskCreationValidationContext::new(&target.store))?;
    let message_backup = target.store.backup_message_files(&task_id)?;

    let facts = match persist_new_task(target, &mut task, initial_messages, write_task) {
        Ok(facts) => facts,
        Err(error) => {
            target
                .store
                .restore_message_files(&task_id, &message_backup)?;
            return Err(error);
        }
    };
    notify_task_updated(target, &facts);
    let snapshot = match options.response_snapshot_tail_limit {
        Some(limit) => Some(build_snapshot(&target.store, &task_id, limit)?),
        None => None,
    };
    Ok(TaskCommitResult {
        outcome: TaskCommitOutcome::Committed(facts),
        response_snapshot: snapshot,
    })
}

pub(super) fn resolve_or_create_new_task(
    target: &TaskMutations,
    task: TaskRecord,
    initial_messages: Vec<NormalizedMessage>,
    options: TaskCommitOptions,
) -> Result<TaskCommitResult, RuntimeError> {
    let _guard = target.lock();
    let TaskLifecycle::New {
        owner_client_instance_id,
    } = &task.lifecycle
    else {
        return Err(RuntimeError::InvalidParams("task.lifecycle".to_string()));
    };
    let existing = target
        .store
        .list_all_task_records_strict()?
        .into_iter()
        .find(|candidate| {
            !candidate.tombstoned
                && matches!(
                    &candidate.lifecycle,
                    TaskLifecycle::New {
                        owner_client_instance_id: candidate_owner
                    } if candidate_owner == owner_client_instance_id
                )
        });
    if let Some(existing) = existing {
        if existing.archived {
            return Err(RuntimeError::Conflict(
                "Client-owned New Task is archived, which violates its lifecycle invariant"
                    .to_string(),
            ));
        }
        if existing.agent_id != task.agent_id
            || existing.workspace_root != task.workspace_root
            || existing.isolation != task.isolation
        {
            return Err(RuntimeError::Conflict(
                "New Task already exists with different Project Context or Agent".to_string(),
            ));
        }
        let response_snapshot = match options.response_snapshot_tail_limit {
            Some(limit) => Some(build_snapshot(&target.store, &existing.task_id, limit)?),
            None => None,
        };
        return Ok(TaskCommitResult {
            outcome: TaskCommitOutcome::Rejected(TaskCommitRejection::NoChange),
            response_snapshot,
        });
    }

    let task_id = task.task_id.clone();
    match target.store.read_task(&task_id) {
        Ok(_) => return Err(RuntimeError::InvalidParams("task_id".to_string())),
        Err(RuntimeError::TaskNotFound(_)) => {}
        Err(error) => return Err(error),
    }
    let message_backup = target.store.backup_message_files(&task_id)?;
    let mut task = task;
    let facts = match persist_new_task(target, &mut task, initial_messages, |store, task| {
        store.write_task(task)
    }) {
        Ok(facts) => facts,
        Err(error) => {
            target
                .store
                .restore_message_files(&task_id, &message_backup)?;
            return Err(error);
        }
    };
    notify_task_updated(target, &facts);
    let response_snapshot = match options.response_snapshot_tail_limit {
        Some(limit) => Some(build_snapshot(&target.store, &task_id, limit)?),
        None => None,
    };
    Ok(TaskCommitResult {
        outcome: TaskCommitOutcome::Committed(facts),
        response_snapshot,
    })
}

#[allow(dead_code)]
pub(super) fn publish_current_task(
    target: &TaskMutations,
    task_id: &str,
) -> Result<(), RuntimeError> {
    let task = target.store.read_task(task_id)?;
    notify_task_updated(
        target,
        &TaskCommitFacts {
            task_id: task.task_id.clone(),
            revision: task.revision,
            committed_task: task,
            delta: None,
        },
    );
    Ok(())
}

pub(super) fn notify_task_updated(target: &TaskMutations, facts: &TaskCommitFacts) {
    match facts.delta.clone() {
        Some(delta) => {
            target
                .notifier
                .task_updated_with_delta(&facts.task_id, facts.revision, delta)
        }
        None => target.notifier.task_updated(&facts.task_id, facts.revision),
    }
}

fn persist_changed_task(
    target: &TaskMutations,
    task: &mut TaskRecord,
    options: TaskCommitOptions,
    delta: Option<crate::task_events::CommittedTaskDelta>,
) -> Result<TaskCommitFacts, RuntimeError> {
    let revision = target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .next_revision_candidate();
    if options.refresh_message_history {
        task.message_history_version = target.store.message_history_version(&task.task_id)?;
    }
    task.task_version += 1;
    task.revision = revision;
    target.store.write_task(task)?;
    target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .commit_revision(revision);
    Ok(TaskCommitFacts {
        task_id: task.task_id.clone(),
        revision,
        committed_task: task.clone(),
        delta,
    })
}

fn persist_new_task(
    target: &TaskMutations,
    task: &mut TaskRecord,
    initial_messages: Vec<NormalizedMessage>,
    write_task: impl FnOnce(&Store, &TaskRecord) -> Result<(), RuntimeError>,
) -> Result<TaskCommitFacts, RuntimeError> {
    let revision = target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .next_revision_candidate();
    target
        .store
        .replace_messages_with_normalized(&task.task_id, initial_messages)?;
    task.message_history_version = target.store.message_history_version(&task.task_id)?;
    task.revision = revision;
    write_task(&target.store, task)?;
    target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .commit_revision(revision);
    Ok(TaskCommitFacts {
        task_id: task.task_id.clone(),
        revision,
        committed_task: task.clone(),
        delta: None,
    })
}

struct VersionFields {
    task_version: u64,
    message_history_version: u64,
    revision: u64,
}

impl VersionFields {
    fn from_task(task: &TaskRecord) -> Self {
        Self {
            task_version: task.task_version,
            message_history_version: task.message_history_version,
            revision: task.revision,
        }
    }
}

fn validate_task_invariants(
    requested_task_id: &str,
    original: &VersionFields,
    task: &TaskRecord,
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
    Ok(())
}
