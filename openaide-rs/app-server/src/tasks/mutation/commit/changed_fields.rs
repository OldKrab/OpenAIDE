use crate::storage::records::TaskRecord;

#[derive(Clone, Copy)]
pub(super) struct ChangedFields {
    pub summary: bool,
    pub lifecycle: bool,
    pub preparation: bool,
    pub agent_config: bool,
    pub agent_commands: bool,
    pub send_capability: bool,
    pub input_capabilities: bool,
    pub context_usage: bool,
    pub current_plan: bool,
    pub message_queue: bool,
    pub removed: bool,
}

pub(super) fn changed_fields(original: &TaskRecord, task: &TaskRecord) -> ChangedFields {
    let preparation = original.preparation != task.preparation;
    let summary = original.title.effective() != task.title.effective()
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
        context_usage: original.context_usage != task.context_usage
            || original.last_turn_usage != task.last_turn_usage,
        current_plan: original.current_plan != task.current_plan,
        message_queue: original.message_queue != task.message_queue,
        removed: !original.tombstoned && task.tombstoned,
    }
}
