use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::{
    ComposerHistoryEntry, ComposerHistoryParams, ComposerHistoryResult, ComposerHistoryScope,
};

use super::{protocol_error_from_runtime, TaskProductApi};

pub(super) fn query(
    api: &TaskProductApi,
    client_instance_id: &ClientInstanceId,
    params: ComposerHistoryParams,
) -> Result<ComposerHistoryResult, ProtocolError> {
    let records = match params.scope {
        ComposerHistoryScope::Task { task_id } => {
            vec![api.read_task_for_client(task_id.as_str(), client_instance_id)?]
        }
        ComposerHistoryScope::Project { project_id } => {
            api.project_resolver.resolve_task_context(&project_id)?;
            api.store
                .list_all_task_records()
                .map_err(protocol_error_from_runtime)?
                .into_iter()
                .filter(|task| task_project_id(task) == project_id.as_str())
                .collect()
        }
    };
    let mut entries = records
        .iter()
        .flat_map(|task| task.composer_history.entries())
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .accepted_at
            .cmp(&left.accepted_at)
            .then_with(|| right.entry_id.cmp(&left.entry_id))
    });
    let mut seen = std::collections::HashSet::new();
    let entries = entries
        .into_iter()
        .filter(|entry| seen.insert(entry.text.as_str()))
        .take(50)
        .map(|entry| ComposerHistoryEntry {
            entry_id: entry.entry_id.clone(),
            text: entry.text.clone(),
            accepted_at: entry.accepted_at.clone(),
        })
        .collect();
    Ok(ComposerHistoryResult { entries })
}

fn task_project_id(task: &crate::storage::records::TaskRecord) -> String {
    crate::projects::task_record_project_id(task)
        .as_str()
        .to_string()
}
