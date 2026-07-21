use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{MessagePage, SettingsSummary, TaskSnapshot};
use crate::storage::records::TaskRecord;
use crate::storage::Store;

const CONSISTENT_SNAPSHOT_READ_ATTEMPTS: usize = 4;

pub fn build_snapshot(
    store: &Store,
    task_id: &str,
    tail_limit: usize,
) -> Result<TaskSnapshot, RuntimeError> {
    for _ in 0..CONSISTENT_SNAPSHOT_READ_ATTEMPTS {
        let task = store.read_task(task_id)?;
        #[cfg(test)]
        store.run_after_task_snapshot_read_hook_for_test();
        let chat = store.tail_page(task_id, tail_limit)?;
        // Separate reads may straddle one atomic journal projection swap. Matching versions prove
        // the Task record and Chat page came from the same committed projection.
        if task.message_history_version != chat.version {
            std::thread::yield_now();
            continue;
        }
        return Ok(snapshot_from_record_and_chat(task, chat));
    }
    Err(RuntimeError::Storage(format!(
        "Task snapshot did not stabilize after {CONSISTENT_SNAPSHOT_READ_ATTEMPTS} reads"
    )))
}

/// Projects one already-committed record and its matching Chat page without another store read.
pub(crate) fn snapshot_from_record_and_chat(task: TaskRecord, chat: MessagePage) -> TaskSnapshot {
    TaskSnapshot {
        active_turn_started_at: task.active_turn_started_at.clone(),
        settings_summary: SettingsSummary {
            agent_id: task.agent_id.clone(),
            isolation: task.isolation,
            model_id: task.model_id.clone(),
        },
        config_options_catalog: task.config_options_catalog.clone(),
        pending_config_change: task.config_mutation.pending.as_ref().map(|pending| {
            crate::protocol::model::PendingTaskConfigChange {
                client_mutation_id: pending.client_mutation_id.clone(),
                config_id: pending.config_id.clone(),
                requested_value: pending.requested_value.clone(),
            }
        }),
        agent_commands_catalog: task.agent_commands_catalog.clone(),
        lifecycle: task.lifecycle.clone(),
        preparation: task.preparation.clone(),
        supports_image_input: task.supports_image_input,
        revision: task.revision,
        task: task.summary(),
        chat,
    }
}
