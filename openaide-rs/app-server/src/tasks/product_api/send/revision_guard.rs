use crate::storage::records::TaskRecord;

pub(super) fn same_send_target(before_attach: &TaskRecord, current: &TaskRecord) -> bool {
    before_attach.task_id == current.task_id
        && before_attach.status == current.status
        && before_attach.message_history_version == current.message_history_version
        && before_attach.agent_id == current.agent_id
        && before_attach.isolation == current.isolation
        && before_attach.workspace_root == current.workspace_root
        && before_attach.first_prompt_sent == current.first_prompt_sent
        && before_attach.agent_session_id == current.agent_session_id
        && before_attach.active_turn_id == current.active_turn_id
        && before_attach.tombstoned == current.tombstoned
        && before_attach.config_options == current.config_options
        && before_attach.model_id == current.model_id
        && before_attach.preparation == current.preparation
}
