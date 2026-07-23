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

mod journal_operations;
mod persist_new;
use journal_operations::journal_operations;
use persist_new::persist_new_task;

pub(super) fn commit_existing_task(
    target: &TaskMutations,
    task_id: &str,
    options: TaskCommitOptions,
    mutation: impl FnOnce(&mut TaskMutationContext<'_>) -> Result<TaskMutationResult, RuntimeError>,
) -> Result<TaskCommitResult, RuntimeError> {
    let _guard = target.lock();
    let mut projection = target.store.task_journal().load(task_id)?;
    let original_task = projection.task.clone();
    let original_version_fields = VersionFields::from_task(&projection.task);
    let mut ctx = TaskMutationContext {
        projection: &mut projection,
        artifact_replacements: Vec::new(),
        terminal_appends: Vec::new(),
        chat_changes: Vec::new(),
        tool_details: Vec::new(),
    };
    let mut mutation_result = mutation(&mut ctx)?;
    // Archived Tasks are immutable history. This transaction-boundary guard also
    // drops late ACP updates that raced with the archive transition.
    if matches!(original_task.lifecycle, TaskLifecycle::Archived)
        && matches!(ctx.task().lifecycle, TaskLifecycle::Archived)
    {
        mutation_result = TaskMutationResult::Unchanged;
    }
    let chat_changes = std::mem::take(&mut ctx.chat_changes);
    let tool_details = std::mem::take(&mut ctx.tool_details);
    let artifact_replacements = std::mem::take(&mut ctx.artifact_replacements);
    let terminal_appends = std::mem::take(&mut ctx.terminal_appends);
    drop(ctx);
    let outcome = match mutation_result {
        TaskMutationResult::Changed => {
            validate_task_invariants(task_id, &original_version_fields, &projection.task)?;
            let facts = persist_changed_projection(
                target,
                &original_task,
                &mut projection,
                options,
                ProjectionChanges {
                    chat: chat_changes,
                    tool_details,
                    artifact_replacements,
                    terminal_appends,
                },
            )?;
            notify_task_changed(target, &facts);
            TaskCommitOutcome::Committed(facts)
        }
        TaskMutationResult::Unchanged | TaskMutationResult::Rejected => {
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
    let facts = persist_new_task(target, &mut task, initial_messages, write_task)?;
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
    let TaskLifecycle::Prepared {
        lease: Some(requesting_client),
    } = &task.lifecycle
    else {
        return Err(RuntimeError::InvalidParams("task.lifecycle".to_string()));
    };
    let records = target.store.list_all_task_records()?;
    if let Some(existing) = records.iter().find(|candidate| {
        !candidate.tombstoned
            && matches!(
                &candidate.lifecycle,
                TaskLifecycle::Prepared { lease: Some(lessee) } if lessee == requesting_client
            )
    }) {
        if existing.agent_id != task.agent_id || existing.workspace_root != task.workspace_root {
            return Err(RuntimeError::Conflict(
                "Release the current Prepared Task before acquiring another context".to_string(),
            ));
        }
        if matches!(
            existing.preparation,
            crate::storage::records::TaskPreparationRecord::Failed {
                native_session_missing: true,
                ..
            }
        ) {
            let original = existing.clone();
            let mut stale = original.clone();
            stale.lifecycle = TaskLifecycle::Prepared { lease: None };
            stale.tombstoned = true;
            stale.updated_at = task.updated_at.clone();
            let stale_facts = persist_changed_task(
                target,
                &original,
                &mut stale,
                TaskCommitOptions::metadata(),
                Vec::new(),
                Vec::new(),
            )?;

            let replacement_task_id = task.task_id.clone();
            let mut replacement = task;
            let facts = persist_new_task(
                target,
                &mut replacement,
                initial_messages,
                |_store, _task| Ok(()),
            )?;
            // Publish only after both records are durable so observers never see
            // the client between Prepared-Task identities.
            notify_task_changed(target, &stale_facts);
            notify_task_changed(target, &facts);
            let response_snapshot = match options.response_snapshot_tail_limit {
                Some(limit) => Some(build_snapshot(&target.store, &replacement_task_id, limit)?),
                None => None,
            };
            crate::logging::info(
                "prepared_task_replaced",
                serde_json::json!({
                    "stale_task_id": stale.task_id,
                    "replacement_task_id": replacement_task_id,
                    "reason": "native_session_missing",
                }),
            );
            return Ok(TaskCommitResult {
                outcome: TaskCommitOutcome::Committed(facts),
                response_snapshot,
            });
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
        leased.lifecycle = TaskLifecycle::Prepared {
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
    let mut task = task;
    let facts = persist_new_task(target, &mut task, initial_messages, |_store, _task| Ok(()))?;
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
        && matches!(task.lifecycle, TaskLifecycle::Prepared { lease: None })
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
        TaskLifecycle::Prepared { lease: Some(lessee) } if lessee == client_instance_id
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
    task.lifecycle = TaskLifecycle::Prepared { lease: None };
    task.updated_at = now.to_string();
    task.last_activity = now.to_string();
    let same_key_already_free = target
        .store
        .list_all_task_records()?
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
        .list_all_task_records()?
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
    for mut task in target.store.list_all_task_records()? {
        let TaskLifecycle::Prepared { lease } = &task.lifecycle else {
            continue;
        };
        if task.tombstoned {
            continue;
        }
        let original = task.clone();
        let invalid = task.status != crate::protocol::model::TaskStatus::Inactive
            || task.active_turn_id.is_some()
            || !target.store.read_messages(&task.task_id)?.is_empty();
        let failed_and_free = (lease.is_none() || clear_leases)
            && matches!(
                task.preparation,
                crate::storage::records::TaskPreparationRecord::Failed { .. }
            );
        if clear_leases {
            task.lifecycle = TaskLifecycle::Prepared { lease: None };
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
        .list_all_task_records()?
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
    dispose_prepared_tasks_matching(target, |task| task.agent_id == agent_id)
}

pub(super) fn dispose_prepared_tasks_for_worktree(
    target: &TaskMutations,
    worktree_id: &str,
) -> Result<Vec<TaskRecord>, RuntimeError> {
    dispose_prepared_tasks_matching(target, |task| {
        task.worktree_id.as_deref() == Some(worktree_id)
    })
}

fn dispose_prepared_tasks_matching(
    target: &TaskMutations,
    matches: impl Fn(&TaskRecord) -> bool,
) -> Result<Vec<TaskRecord>, RuntimeError> {
    let _guard = target.lock();
    let mut disposed = Vec::new();
    for mut task in target.store.list_all_task_records()? {
        if task.tombstoned
            || !matches(&task)
            || !matches!(task.lifecycle, TaskLifecycle::Prepared { .. })
        {
            continue;
        }
        let original = task.clone();
        task.lifecycle = TaskLifecycle::Prepared { lease: None };
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
    chat: Vec<CommittedChatChange>,
    tool_details: Vec<ToolDetailUpdate>,
) -> Result<TaskCommitFacts, RuntimeError> {
    let mut projection = target.store.task_journal().load(&task.task_id)?;
    projection.task = task.clone();
    let facts = persist_changed_projection(
        target,
        original,
        &mut projection,
        options,
        ProjectionChanges {
            chat,
            tool_details,
            artifact_replacements: Vec::new(),
            terminal_appends: Vec::new(),
        },
    )?;
    *task = facts.committed_task.clone();
    Ok(facts)
}

struct ProjectionChanges {
    chat: Vec<CommittedChatChange>,
    tool_details: Vec<ToolDetailUpdate>,
    artifact_replacements: Vec<crate::storage::tool_artifacts::PersistedToolDetail>,
    terminal_appends: Vec<crate::storage::task_journal::ToolTerminalAppend>,
}

fn persist_changed_projection(
    target: &TaskMutations,
    original: &TaskRecord,
    projection: &mut crate::storage::task_journal::TaskProjection,
    options: TaskCommitOptions,
    changes: ProjectionChanges,
) -> Result<TaskCommitFacts, RuntimeError> {
    let ProjectionChanges {
        mut chat,
        mut tool_details,
        artifact_replacements,
        terminal_appends,
    } = changes;
    #[cfg(test)]
    if target.store.take_task_write_failure_for_test() {
        return Err(RuntimeError::Storage(
            "injected Task record write failure".to_string(),
        ));
    }
    let runtime_revision = target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .next_revision_candidate();
    if options.refresh_message_history {
        projection.task.message_history_version = projection.message_meta.version;
    }
    projection.task.task_version += 1;
    projection.task.revision = original
        .revision
        .checked_add(1)
        .ok_or_else(|| RuntimeError::Internal("Task revision overflow".to_string()))?;
    if projection.task.message_history_version != original.message_history_version
        && chat.is_empty()
    {
        chat.push(CommittedChatChange::Replace);
    }
    let fields = changed_fields(original, &projection.task);
    let has_messages = !projection.messages.is_empty();
    let projected = crate::snapshots::task_snapshot::project_committed_task_state(
        projection.task.clone(),
        has_messages,
    )
    .map_err(|error| RuntimeError::Internal(error.message))?;
    let navigation = navigation_change(original, &projection.task, fields.summary, &projected.task);
    let lifecycle = (original.lifecycle != projection.task.lifecycle).then(|| {
        openaide_app_server_protocol::task::TaskLifecycleChanged {
            previous_lifecycle: crate::snapshots::project_task_lifecycle(&original.lifecycle),
            task: crate::snapshots::project_task_summary(projection.task.clone()),
        }
    });
    let committed_task = projection.task.clone();
    let journal_operations = journal_operations(projection, &chat)?;
    // Build every fallible publication value before the durability barrier.
    // Once the receipt resolves, the remaining path only installs revision
    // facts and returns already-owned data.
    let changes = project_committed_changes(target, &projected, projection, fields, chat)?;
    let committed = target
        .store
        .task_journal()
        .submit(
            crate::storage::task_journal::TaskWrite::barrier_operations_with_artifacts(
                committed_task.task_id.clone(),
                journal_operations,
                artifact_replacements
                    .into_iter()
                    .map(
                        |detail| crate::storage::task_journal::ToolArtifactReplacement {
                            artifact_id: detail.artifact_id,
                            details: detail.details,
                        },
                    )
                    .collect(),
                terminal_appends,
            ),
        )?
        .wait()?;
    for detail in &mut tool_details {
        if let Some(change) = committed
            .artifact_changes
            .iter()
            .find(|change| change.artifact_id == detail.artifact_id)
        {
            detail.details.revision = change.artifact_sequence;
            detail.terminal_appends = change.terminal_appends.clone();
        }
    }
    target
        .runtime_state
        .lock()
        .expect("runtime state poisoned")
        .commit_revision(runtime_revision);
    Ok(TaskCommitFacts {
        task_id: committed_task.task_id.clone(),
        revision: committed_task.revision,
        committed_task,
        change: CommittedTaskChange {
            changes,
            tool_details,
            navigation,
            lifecycle,
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
            task: Box::new(summary.clone()),
        }),
        (true, true) if summary_changed => Some(TaskNavigationChange::Upsert {
            task: Box::new(summary.clone()),
        }),
        _ => None,
    }
}

fn project_committed_changes(
    target: &TaskMutations,
    task: &openaide_app_server_protocol::snapshot::TaskSnapshot,
    projection: &crate::storage::task_journal::TaskProjection,
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
                let total = projection.messages.len();
                let requested_start =
                    total.saturating_sub(ChatHistoryPolicy::default().task_snapshot_tail_limit());
                let start = crate::storage::message_store::chat_page_start(
                    &projection.messages,
                    requested_start,
                    total,
                );
                let page = target
                    .store
                    .page_from_projection(projection, start, total)?;
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
    matches!(task.lifecycle, TaskLifecycle::Open) && !task.tombstoned
}

struct VersionFields {
    task_version: u64,
    message_history_version: u64,
    revision: u64,
    bound_native_session_id: Option<String>,
}

impl VersionFields {
    fn from_task(task: &TaskRecord) -> Self {
        Self {
            task_version: task.task_version,
            message_history_version: task.message_history_version,
            revision: task.revision,
            bound_native_session_id: task.agent_session_id.clone(),
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
    if original.bound_native_session_id.is_some()
        && task.agent_session_id != original.bound_native_session_id
    {
        return Err(RuntimeError::Internal(
            "task mutation changed bound Native Session identity".to_string(),
        ));
    }
    Ok(())
}
