use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{SettingsSummary, TaskSnapshot};
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
        // Message files are committed before task.json. Matching versions prove the snapshot did
        // not cross that commit boundary; a mismatch is transient while the Task write catches up.
        if task.message_history_version != chat.version {
            std::thread::yield_now();
            continue;
        }
        return Ok(TaskSnapshot {
            settings_summary: SettingsSummary {
                agent_id: task.agent_id.clone(),
                isolation: task.isolation,
                model_id: task.model_id.clone(),
                config_options: task.config_options.clone(),
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
            permissions: chat
                .items
                .iter()
                .filter_map(|item| match &item.message {
                    crate::protocol::model::NormalizedMessage::Permission { .. } => {
                        Some(item.message.clone())
                    }
                    _ => None,
                })
                .collect(),
            revision: task.revision,
            task: task.summary(),
            chat,
        });
    }
    Err(RuntimeError::Storage(format!(
        "Task snapshot did not stabilize after {CONSISTENT_SNAPSHOT_READ_ATTEMPTS} reads"
    )))
}
