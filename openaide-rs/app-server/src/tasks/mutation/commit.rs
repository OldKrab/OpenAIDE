use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::storage::Store;
use crate::task_events::{
    CommittedChatChange, CommittedTaskChange, TaskFieldChanges, TaskNavigationChange,
    ToolDetailUpdate,
};
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
    let original_task = task.clone();
    let original_version_fields = VersionFields::from_task(&task);
    let message_backup = target.store.backup_message_files(task_id)?;
    let rollback = || target.store.restore_message_files(task_id, &message_backup);
    let mut ctx = TaskMutationContext {
        store: &target.store,
        task: &mut task,
        chat_changes: Vec::new(),
        tool_details: Vec::new(),
    };
    let mutation_result = match mutation(&mut ctx) {
        Ok(result) => result,
        Err(error) => {
            rollback()?;
            return Err(error);
        }
    };
    let chat_changes = std::mem::take(&mut ctx.chat_changes);
    let tool_details = std::mem::take(&mut ctx.tool_details);
    drop(ctx);
    let outcome = match mutation_result {
        TaskMutationResult::Changed => {
            if let Err(error) = validate_task_invariants(task_id, &original_version_fields, &task) {
                rollback()?;
                return Err(error);
            }
            let facts = match persist_changed_task(
                target,
                &original_task,
                &mut task,
                options,
                chat_changes,
                tool_details,
            ) {
                Ok(facts) => facts,
                Err(error) => {
                    rollback()?;
                    return Err(error);
                }
            };
            notify_task_changed(target, &facts);
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
    notify_task_changed(target, &facts);
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
    notify_task_changed(target, &facts);
    let response_snapshot = match options.response_snapshot_tail_limit {
        Some(limit) => Some(build_snapshot(&target.store, &task_id, limit)?),
        None => None,
    };
    Ok(TaskCommitResult {
        outcome: TaskCommitOutcome::Committed(facts),
        response_snapshot,
    })
}

pub(super) fn notify_task_changed(target: &TaskMutations, facts: &TaskCommitFacts) {
    target
        .notifier
        .task_changed(&facts.task_id, facts.revision, facts.change.clone());
}

fn persist_changed_task(
    target: &TaskMutations,
    original: &TaskRecord,
    task: &mut TaskRecord,
    options: TaskCommitOptions,
    mut chat: Vec<CommittedChatChange>,
    tool_details: Vec<ToolDetailUpdate>,
) -> Result<TaskCommitFacts, RuntimeError> {
    let runtime_revision = target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .next_revision_candidate();
    if options.refresh_message_history {
        task.message_history_version = target.store.message_history_version(&task.task_id)?;
    }
    task.task_version += 1;
    task.revision = original
        .revision
        .checked_add(1)
        .ok_or_else(|| RuntimeError::Internal("Task revision overflow".to_string()))?;
    if task.message_history_version != original.message_history_version && chat.is_empty() {
        chat.push(CommittedChatChange::Replace);
    }
    target.store.write_task(task)?;
    target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .commit_revision(runtime_revision);
    let fields = changed_fields(original, task);
    let navigation = navigation_change(original, task, fields.summary);
    Ok(TaskCommitFacts {
        task_id: task.task_id.clone(),
        revision: task.revision,
        committed_task: task.clone(),
        change: CommittedTaskChange {
            fields,
            chat,
            tool_details,
            navigation,
        },
    })
}

fn persist_new_task(
    target: &TaskMutations,
    task: &mut TaskRecord,
    initial_messages: Vec<NormalizedMessage>,
    write_task: impl FnOnce(&Store, &TaskRecord) -> Result<(), RuntimeError>,
) -> Result<TaskCommitFacts, RuntimeError> {
    let runtime_revision = target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .next_revision_candidate();
    target
        .store
        .replace_messages_with_normalized(&task.task_id, initial_messages)?;
    task.message_history_version = target.store.message_history_version(&task.task_id)?;
    task.revision = 1;
    write_task(&target.store, task)?;
    target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .commit_revision(runtime_revision);
    let visible = navigation_member(task);
    Ok(TaskCommitFacts {
        task_id: task.task_id.clone(),
        revision: task.revision,
        committed_task: task.clone(),
        change: CommittedTaskChange {
            fields: TaskFieldChanges {
                summary: true,
                lifecycle: true,
                preparation: true,
                agent_config: true,
                agent_commands: true,
                send_capability: true,
                removed: task.tombstoned,
            },
            chat: vec![CommittedChatChange::Replace],
            tool_details: Vec::new(),
            navigation: if visible {
                TaskNavigationChange::Upsert
            } else {
                TaskNavigationChange::None
            },
        },
    })
}

fn changed_fields(original: &TaskRecord, task: &TaskRecord) -> TaskFieldChanges {
    let preparation = original.preparation != task.preparation;
    let summary = original.title != task.title
        || original.status != task.status
        || original.unread != task.unread
        || original.updated_at != task.updated_at
        || original.last_activity != task.last_activity
        || original.agent_id != task.agent_id
        || original.workspace_root != task.workspace_root
        || original.message_history_version != task.message_history_version
        || preparation;
    TaskFieldChanges {
        summary,
        lifecycle: original.lifecycle != task.lifecycle,
        preparation,
        agent_config: preparation
            || original.config_options != task.config_options
            || original.config_options_catalog != task.config_options_catalog
            || original.config_mutation != task.config_mutation
            || original.model_id != task.model_id,
        agent_commands: preparation
            || original.agent_commands_catalog != task.agent_commands_catalog,
        send_capability: preparation || original.status != task.status,
        removed: !original.tombstoned && task.tombstoned,
    }
}

fn navigation_change(
    original: &TaskRecord,
    task: &TaskRecord,
    summary_changed: bool,
) -> TaskNavigationChange {
    match (navigation_member(original), navigation_member(task)) {
        (true, false) => TaskNavigationChange::Remove,
        (false, true) => TaskNavigationChange::Upsert,
        (true, true) if summary_changed => TaskNavigationChange::Upsert,
        _ => TaskNavigationChange::None,
    }
}

fn navigation_member(task: &TaskRecord) -> bool {
    matches!(task.lifecycle, TaskLifecycle::Visible) && !task.archived && !task.tombstoned
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
