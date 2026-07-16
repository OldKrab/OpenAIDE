use openaide_app_server_protocol::events::{TaskChanges, TaskChatChange, TaskNavigationChange};
use openaide_app_server_protocol::ids::ClientInstanceId;
use std::collections::HashSet;

use crate::chat_history::ChatHistoryPolicy;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::storage::Store;
use crate::task_events::{CommittedChatChange, CommittedTaskChange, ToolDetailUpdate};
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
            target.store.discard_message_files_backup(&message_backup);
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
    target.store.discard_message_files_backup(&message_backup);
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

pub(super) fn acquire_prepared_task(
    target: &TaskMutations,
    task: TaskRecord,
    initial_messages: Vec<NormalizedMessage>,
    options: TaskCommitOptions,
) -> Result<TaskCommitResult, RuntimeError> {
    let _guard = target.lock();
    let TaskLifecycle::New {
        lease: Some(requesting_client),
    } = &task.lifecycle
    else {
        return Err(RuntimeError::InvalidParams("task.lifecycle".to_string()));
    };
    let records = target.store.list_all_task_records_strict()?;
    if let Some(existing) = records.iter().find(|candidate| {
        !candidate.tombstoned
            && matches!(
                &candidate.lifecycle,
                TaskLifecycle::New { lease: Some(lessee) } if lessee == requesting_client
            )
    }) {
        if existing.archived {
            return Err(RuntimeError::Conflict(
                "Leased Prepared Task is archived, which violates its lifecycle invariant"
                    .to_string(),
            ));
        }
        if existing.agent_id != task.agent_id || existing.workspace_root != task.workspace_root {
            return Err(RuntimeError::Conflict(
                "Release the current Prepared Task before acquiring another context".to_string(),
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

    if let Some(existing) = records.into_iter().find(|candidate| {
        is_reusable_prepared_task(target, candidate)
            && candidate.agent_id == task.agent_id
            && candidate.workspace_root == task.workspace_root
    }) {
        let original = existing.clone();
        let mut leased = existing;
        leased.lifecycle = TaskLifecycle::New {
            lease: Some(requesting_client.clone()),
        };
        let facts = persist_changed_task(
            target,
            &original,
            &mut leased,
            options,
            Vec::new(),
            Vec::new(),
        )?;
        notify_task_changed(target, &facts);
        let response_snapshot = match options.response_snapshot_tail_limit {
            Some(limit) => Some(build_snapshot(&target.store, &leased.task_id, limit)?),
            None => None,
        };
        return Ok(TaskCommitResult {
            outcome: TaskCommitOutcome::Committed(facts),
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
    target.store.discard_message_files_backup(&message_backup);
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

fn is_reusable_prepared_task(target: &TaskMutations, task: &TaskRecord) -> bool {
    !task.tombstoned
        && !task.archived
        && matches!(task.lifecycle, TaskLifecycle::New { lease: None })
        && matches!(
            task.preparation,
            crate::storage::records::TaskPreparationRecord::Ready
        )
        && task.status == crate::protocol::model::TaskStatus::Inactive
        && task.active_turn_id.is_none()
        && target
            .store
            .read_messages(&task.task_id)
            .is_ok_and(|messages| messages.is_empty())
}

const FREE_PREPARED_TASK_CAP: usize = 8;

pub(super) fn release_prepared_task(
    target: &TaskMutations,
    client_instance_id: &ClientInstanceId,
    task_id: &str,
    now: &str,
) -> Result<Vec<TaskRecord>, RuntimeError> {
    let _guard = target.lock();
    let mut task = match target.store.read_task(task_id) {
        Ok(task) => task,
        Err(RuntimeError::TaskNotFound(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let leased_by_client = matches!(
        &task.lifecycle,
        TaskLifecycle::New { lease: Some(lessee) } if lessee == client_instance_id
    );
    if !leased_by_client {
        return Ok(Vec::new());
    }
    if task.status != crate::protocol::model::TaskStatus::Inactive
        || task.active_turn_id.is_some()
        || !target.store.read_messages(task_id)?.is_empty()
    {
        return Err(RuntimeError::Conflict(
            "Only an empty inactive Prepared Task can be released".to_string(),
        ));
    }

    let original = task.clone();
    task.lifecycle = TaskLifecycle::New { lease: None };
    task.updated_at = now.to_string();
    task.last_activity = now.to_string();
    let same_key_already_free =
        target
            .store
            .list_all_task_records_strict()?
            .iter()
            .any(|candidate| {
                candidate.task_id != task.task_id
                    && candidate.agent_id == task.agent_id
                    && candidate.workspace_root == task.workspace_root
                    && is_reusable_prepared_task(target, candidate)
            });
    let failed = matches!(
        task.preparation,
        crate::storage::records::TaskPreparationRecord::Failed { .. }
    );
    if failed || same_key_already_free {
        task.tombstoned = true;
    }
    let facts = persist_changed_task(
        target,
        &original,
        &mut task,
        TaskCommitOptions::metadata(),
        Vec::new(),
        Vec::new(),
    )?;
    notify_task_changed(target, &facts);
    let released_task_id = task.task_id.clone();
    let release_outcome = if task.tombstoned {
        "disposed"
    } else {
        "retained"
    };
    let release_reason = if failed {
        "preparation_failed"
    } else if same_key_already_free {
        "duplicate_pool_key"
    } else {
        "free_pool_entry"
    };
    let mut disposed = task
        .tombstoned
        .then_some(task)
        .into_iter()
        .collect::<Vec<_>>();

    let mut free = target
        .store
        .list_all_task_records_strict()?
        .into_iter()
        .filter(|candidate| is_reusable_prepared_task(target, candidate))
        .collect::<Vec<_>>();
    free.sort_by(|left, right| {
        left.updated_at
            .cmp(&right.updated_at)
            .then_with(|| left.task_id.cmp(&right.task_id))
    });
    let free_count = free.len();
    let overflow = free_count.saturating_sub(FREE_PREPARED_TASK_CAP);
    for mut evicted in free.into_iter().take(overflow) {
        let original = evicted.clone();
        evicted.tombstoned = true;
        let facts = persist_changed_task(
            target,
            &original,
            &mut evicted,
            TaskCommitOptions::metadata(),
            Vec::new(),
            Vec::new(),
        )?;
        notify_task_changed(target, &facts);
        crate::logging::info(
            "prepared_task_evicted",
            serde_json::json!({
                "task_id": evicted.task_id,
                "reason": "global_free_lru_cap",
                "free_count_before": free_count,
            }),
        );
        disposed.push(evicted);
    }
    crate::logging::info(
        "prepared_task_released",
        serde_json::json!({
            "task_id": released_task_id,
            "outcome": release_outcome,
            "reason": release_reason,
            "free_count_after": free_count - overflow,
        }),
    );
    Ok(disposed)
}

pub(super) fn reconcile_prepared_task_pool(
    target: &TaskMutations,
    clear_leases: bool,
) -> Result<Vec<TaskRecord>, RuntimeError> {
    let _guard = target.lock();
    let mut disposed = Vec::new();
    for mut task in target.store.list_all_task_records_strict()? {
        let TaskLifecycle::New { lease } = &task.lifecycle else {
            continue;
        };
        if task.tombstoned {
            continue;
        }
        let original = task.clone();
        let invalid = task.archived
            || task.status != crate::protocol::model::TaskStatus::Inactive
            || task.active_turn_id.is_some()
            || !target.store.read_messages(&task.task_id)?.is_empty();
        let failed_and_free = (lease.is_none() || clear_leases)
            && matches!(
                task.preparation,
                crate::storage::records::TaskPreparationRecord::Failed { .. }
            );
        if clear_leases {
            task.lifecycle = TaskLifecycle::New { lease: None };
        }
        if invalid || failed_and_free {
            task.tombstoned = true;
        }
        if task.lifecycle == original.lifecycle && task.tombstoned == original.tombstoned {
            continue;
        }
        let facts = persist_changed_task(
            target,
            &original,
            &mut task,
            TaskCommitOptions::metadata(),
            Vec::new(),
            Vec::new(),
        )?;
        notify_task_changed(target, &facts);
        if task.tombstoned {
            disposed.push(task);
        }
    }

    let mut free = target
        .store
        .list_all_task_records_strict()?
        .into_iter()
        .filter(|candidate| is_reusable_prepared_task(target, candidate))
        .collect::<Vec<_>>();
    free.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.task_id.cmp(&left.task_id))
    });
    let mut kept_keys = HashSet::new();
    let mut kept_count = 0usize;
    for mut candidate in free {
        let key = (candidate.agent_id.clone(), candidate.workspace_root.clone());
        let duplicate_key = kept_keys.contains(&key);
        if kept_count < FREE_PREPARED_TASK_CAP && !duplicate_key {
            kept_keys.insert(key);
            kept_count += 1;
            continue;
        }
        let original = candidate.clone();
        candidate.tombstoned = true;
        let facts = persist_changed_task(
            target,
            &original,
            &mut candidate,
            TaskCommitOptions::metadata(),
            Vec::new(),
            Vec::new(),
        )?;
        notify_task_changed(target, &facts);
        crate::logging::info(
            "prepared_task_evicted",
            serde_json::json!({
                "task_id": candidate.task_id,
                "reason": if duplicate_key { "duplicate_pool_key" } else { "global_free_lru_cap" },
            }),
        );
        disposed.push(candidate);
    }
    Ok(disposed)
}

pub(super) fn dispose_prepared_tasks_for_agent(
    target: &TaskMutations,
    agent_id: &str,
) -> Result<Vec<TaskRecord>, RuntimeError> {
    let _guard = target.lock();
    let mut disposed = Vec::new();
    for mut task in target.store.list_all_task_records_strict()? {
        if task.tombstoned
            || task.agent_id != agent_id
            || !matches!(task.lifecycle, TaskLifecycle::New { .. })
        {
            continue;
        }
        let original = task.clone();
        task.lifecycle = TaskLifecycle::New { lease: None };
        task.tombstoned = true;
        let facts = persist_changed_task(
            target,
            &original,
            &mut task,
            TaskCommitOptions::metadata(),
            Vec::new(),
            Vec::new(),
        )?;
        notify_task_changed(target, &facts);
        disposed.push(task);
    }
    Ok(disposed)
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
    let fields = changed_fields(original, task);
    let has_messages = target.store.message_history_has_messages(&task.task_id)?;
    let projected =
        crate::snapshots::task_snapshot::project_committed_task_state(task.clone(), has_messages)
            .map_err(|error| RuntimeError::Internal(error.message))?;
    let navigation = navigation_change(original, task, fields.summary, &projected.task);
    let changes = project_committed_changes(target, &projected, fields, chat)?;
    target.store.write_task(task)?;
    target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .commit_revision(runtime_revision);
    Ok(TaskCommitFacts {
        task_id: task.task_id.clone(),
        revision: task.revision,
        committed_task: task.clone(),
        change: CommittedTaskChange {
            changes,
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
    let has_messages = target.store.message_history_has_messages(&task.task_id)?;
    let projected =
        crate::snapshots::task_snapshot::project_committed_task_state(task.clone(), has_messages)
            .map_err(|error| RuntimeError::Internal(error.message))?;
    let fields = ChangedFields {
        summary: true,
        lifecycle: true,
        preparation: true,
        agent_config: true,
        agent_commands: true,
        send_capability: true,
        input_capabilities: true,
        removed: task.tombstoned,
    };
    let changes = project_committed_changes(
        target,
        &projected,
        fields,
        vec![CommittedChatChange::Replace],
    )?;
    let navigation = navigation_member(task).then(|| TaskNavigationChange::Upsert {
        task: projected.task.clone(),
    });
    write_task(&target.store, task)?;
    target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .commit_revision(runtime_revision);
    Ok(TaskCommitFacts {
        task_id: task.task_id.clone(),
        revision: task.revision,
        committed_task: task.clone(),
        change: CommittedTaskChange {
            changes,
            tool_details: Vec::new(),
            navigation,
        },
    })
}

#[derive(Clone, Copy)]
struct ChangedFields {
    summary: bool,
    lifecycle: bool,
    preparation: bool,
    agent_config: bool,
    agent_commands: bool,
    send_capability: bool,
    input_capabilities: bool,
    removed: bool,
}

fn changed_fields(original: &TaskRecord, task: &TaskRecord) -> ChangedFields {
    let preparation = original.preparation != task.preparation;
    let summary = original.title != task.title
        || original.status != task.status
        || original.unread != task.unread
        || original.attention != task.attention
        || original.updated_at != task.updated_at
        || original.last_activity != task.last_activity
        || original.agent_id != task.agent_id
        || original.workspace_root != task.workspace_root
        || original.message_history_version != task.message_history_version
        || preparation;
    ChangedFields {
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
        input_capabilities: original.supports_image_input != task.supports_image_input,
        removed: !original.tombstoned && task.tombstoned,
    }
}

fn navigation_change(
    original: &TaskRecord,
    task: &TaskRecord,
    summary_changed: bool,
    summary: &openaide_app_server_protocol::snapshot::TaskSummary,
) -> Option<TaskNavigationChange> {
    match (navigation_member(original), navigation_member(task)) {
        (true, false) => Some(TaskNavigationChange::Remove {
            task_id: task.task_id.clone().into(),
        }),
        (false, true) => Some(TaskNavigationChange::Upsert {
            task: summary.clone(),
        }),
        (true, true) if summary_changed => Some(TaskNavigationChange::Upsert {
            task: summary.clone(),
        }),
        _ => None,
    }
}

fn project_committed_changes(
    target: &TaskMutations,
    task: &openaide_app_server_protocol::snapshot::TaskSnapshot,
    fields: ChangedFields,
    chat: Vec<CommittedChatChange>,
) -> Result<TaskChanges, RuntimeError> {
    let mut projected_chat = Vec::with_capacity(chat.len());
    for change in chat {
        projected_chat.push(match change {
            CommittedChatChange::Append { item } => TaskChatChange::Append { item },
            CommittedChatChange::Upsert { item } => TaskChatChange::Upsert { item },
            CommittedChatChange::AppendText { message_id, text } => {
                TaskChatChange::AppendText { message_id, text }
            }
            CommittedChatChange::Replace => {
                let page = target.store.tail_page(
                    task.task.task_id.as_str(),
                    ChatHistoryPolicy::default().task_snapshot_tail_limit(),
                )?;
                TaskChatChange::Replace {
                    chat: crate::snapshots::task_snapshot::project_chat_page(page),
                }
            }
        });
    }
    Ok(TaskChanges {
        task: fields.summary.then(|| task.task.clone()),
        active_turn_started_at: fields.summary.then(|| task.active_turn_started_at.clone()),
        lifecycle: fields.lifecycle.then_some(task.lifecycle),
        preparation: fields.preparation.then(|| task.preparation.clone()),
        agent_config: fields.agent_config.then(|| task.agent_config.clone()),
        agent_commands: fields.agent_commands.then(|| task.agent_commands.clone()),
        send_capability: fields.send_capability.then(|| task.send_capability.clone()),
        input_capabilities: fields
            .input_capabilities
            .then_some(task.input_capabilities)
            .flatten(),
        chat: projected_chat,
        removed: fields.removed,
    })
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
