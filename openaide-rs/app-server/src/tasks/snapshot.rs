use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{SettingsSummary, TaskSnapshot};
use crate::storage::Store;

pub fn build_snapshot(
    store: &Store,
    task_id: &str,
    tail_limit: usize,
) -> Result<TaskSnapshot, RuntimeError> {
    let task = store.read_task(task_id)?;
    let chat = store.tail_page(task_id, tail_limit)?;
    Ok(TaskSnapshot {
        settings_summary: SettingsSummary {
            agent_id: task.agent_id.clone(),
            isolation: task.isolation,
            model_id: task.model_id.clone(),
            config_options: task.config_options.clone(),
        },
        config_options_catalog: task.config_options_catalog.clone(),
        agent_commands_catalog: task.agent_commands_catalog.clone(),
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
    })
}
