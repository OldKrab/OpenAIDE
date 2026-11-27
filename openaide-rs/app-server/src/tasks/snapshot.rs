use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{MessagePage, SettingsSummary, TaskSnapshot};
use crate::storage::records::TaskRecord;
use crate::storage::Store;

pub fn build_snapshot(
    store: &Store,
    task_id: &str,
    tail_limit: usize,
) -> Result<TaskSnapshot, RuntimeError> {
    let (task, chat) = store.read_task_with_tail(task_id, tail_limit)?;
    Ok(snapshot_from_record_and_chat(task, chat))
}

/// Projects one already-committed record and its matching Chat page without another store read.
pub(crate) fn snapshot_from_record_and_chat(task: TaskRecord, chat: MessagePage) -> TaskSnapshot {
    let context_usage = task.context_usage.clone().map(|mut usage| {
        usage.last_turn = task.last_turn_usage.clone();
        usage
    });
    TaskSnapshot {
        permission_policy: task.permission_policy,
        active_turn_started_at: task.active_turn_started_at.clone(),
        settings_summary: SettingsSummary {
            agent_id: task.agent_id.clone(),
            isolation: task.isolation,
            model_id: task.model_id.clone(),
        },
        config_options_catalog: task.config_options_catalog.clone(),
        native_session_data_freshness: task.native_session_data_freshness,
        native_session_reload_requirement: task.native_session_reload_requirement.clone(),
        pending_config_change: task.config_mutation.pending.as_ref().map(|pending| {
            crate::protocol::model::PendingTaskConfigChange {
                client_mutation_id: pending.client_mutation_id.clone(),
                config_id: pending.config_id.clone(),
                requested_value: pending.requested_value.clone(),
            }
        }),
        config_preferences: task.config_mutation.preferences.clone(),
        agent_commands_catalog: task.agent_commands_catalog.clone(),
        lifecycle: task.lifecycle.clone(),
        message_queue: task.message_queue.clone(),
        preparation: task.preparation.clone(),
        supports_image_input: task.supports_image_input,
        context_usage,
        current_plan: task.current_plan.clone(),
        revision: task.revision,
        task: task.summary(),
        chat,
    }
}
