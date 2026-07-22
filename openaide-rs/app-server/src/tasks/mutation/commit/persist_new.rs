use openaide_app_server_protocol::events::TaskNavigationChange;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::NormalizedMessage;
use crate::storage::records::TaskRecord;
use crate::storage::Store;
use crate::task_events::{CommittedChatChange, CommittedTaskChange};

use super::{navigation_member, project_committed_changes, ChangedFields};
use crate::tasks::mutation::{TaskCommitFacts, TaskMutations};

pub(super) fn persist_new_task(
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
    let mut stored_messages = Vec::with_capacity(initial_messages.len());
    let mut artifact_replacements = Vec::new();
    for (index, mut message) in initial_messages.into_iter().enumerate() {
        artifact_replacements.extend(crate::storage::tool_artifacts::extract_tool_artifacts(
            &mut message,
        ));
        let sequence = index as u64 + 1;
        let identity = message.identity();
        stored_messages.push(crate::storage::records::StoredMessage {
            sequence,
            chat: crate::protocol::model::ChatMessage {
                cursor: crate::storage::cursor::from_sequence(sequence),
                message_id: identity.clone(),
                identity,
                message_type: message.message_type().to_string(),
                message,
            },
        });
    }
    let message_meta = crate::storage::records::MessageMeta {
        task_id: task.task_id.clone(),
        version: stored_messages.len() as u64,
        message_count: stored_messages.len() as u64,
        local_history_updated_at: crate::time::now_string(),
        first_cursor: stored_messages
            .first()
            .map(|message| message.chat.cursor.clone()),
        last_cursor: stored_messages
            .last()
            .map(|message| message.chat.cursor.clone()),
    };
    task.message_history_version = message_meta.version;
    task.revision = 1;
    let projected = crate::snapshots::task_snapshot::project_committed_task_state(
        task.clone(),
        !stored_messages.is_empty(),
    )
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
    let navigation = navigation_member(task).then(|| TaskNavigationChange::Upsert {
        task: Box::new(projected.task.clone()),
    });
    // Test-only injected writers validate failure before any durable frame.
    write_task(&target.store, task)?;
    let projection = crate::storage::task_journal::TaskProjection {
        task: task.clone(),
        messages: stored_messages,
        message_meta,
        artifact_heads: Default::default(),
    };
    let changes = project_committed_changes(
        target,
        &projected,
        &projection,
        fields,
        vec![CommittedChatChange::Replace],
    )?;
    target
        .store
        .task_journal()
        .submit(
            crate::storage::task_journal::TaskWrite::barrier_create_with_artifacts(
                projection,
                artifact_replacements
                    .into_iter()
                    .map(
                        |detail| crate::storage::task_journal::ToolArtifactReplacement {
                            artifact_id: detail.artifact_id,
                            details: detail.details,
                        },
                    )
                    .collect(),
            ),
        )?
        .wait()?;
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
