//! Publishes what the Task journal worker committed without a waiting caller:
//! streamed Chat text, terminal output, and Subagent changes (ADR-0076). It runs
//! on the store's single dispatch thread, so publication keeps commit order.

use openaide_app_server_protocol::events::TaskChatChange;

use crate::snapshots::task_snapshot::{
    project_agent_plan, project_chat_item, project_subagent_catalog,
};
use crate::storage::subagents::{CommittedSubagentBatch, CommittedSubagentChange};
use crate::storage::task_journal::CommittedTaskBatch;
use crate::task_events::TaskUpdateNotifier;

use super::PendingStreamText;

pub(super) fn publish(
    notifier: &TaskUpdateNotifier,
    pending_text: &PendingStreamText,
    committed: CommittedTaskBatch,
) {
    if let Some(subagent) = committed.subagent {
        publish_subagent_batch(notifier, &committed.task_id, subagent);
        return;
    }
    if !committed.chat_text_appends.is_empty() {
        let chat = committed
            .chat_text_appends
            .into_iter()
            .map(|append| TaskChatChange::AppendText {
                message_id: append.message_id.into(),
                text: append.text,
            })
            .collect();
        notifier.streamed_chat_changed(&committed.task_id, committed.task_revision, chat);
    }
    for change in committed.artifact_changes {
        // A structured replacement and its same-revision terminal bytes must
        // remain one atomic synchronous Tool delta. Other stream appends in the
        // physical batch keep their async owner.
        let synchronously_owned = committed.task_snapshot_changed
            && committed
                .replaced_artifact_ids
                .contains(&change.artifact_id);
        if !synchronously_owned && !change.terminal_appends.is_empty() {
            notifier.tool_detail_changed(
                &committed.task_id,
                change.artifact_id,
                change.artifact_sequence,
                change.terminal_appends,
            );
        }
    }
    pending_text.complete(&committed.task_id, committed.streamed_text_write_count);
}

/// Publishes each history revision as its own delta, so a client applies them
/// in order and re-reads a history when one is missing, then the catalog.
fn publish_subagent_batch(
    notifier: &TaskUpdateNotifier,
    task_id: &str,
    batch: CommittedSubagentBatch,
) {
    for revision in batch.revisions {
        let Some(change) = revision.change else {
            // A watching client meets a revision gap and re-reads the history.
            crate::logging::warn(
                "subagent_history_change_unpublished",
                serde_json::json!({
                    "task_id": task_id,
                    "subagent_id": revision.subagent_id,
                    "revision": revision.revision,
                }),
            );
            continue;
        };
        let (chat, current_plan) = match change {
            CommittedSubagentChange::Append(message) => (
                vec![TaskChatChange::Append {
                    item: project_chat_item(&message),
                }],
                None,
            ),
            CommittedSubagentChange::Upsert(message) => (
                vec![TaskChatChange::Upsert {
                    item: project_chat_item(&message),
                }],
                None,
            ),
            CommittedSubagentChange::AppendText { message_id, text } => (
                vec![TaskChatChange::AppendText {
                    message_id: message_id.into(),
                    text,
                }],
                None,
            ),
            CommittedSubagentChange::Plan(plan) => (Vec::new(), Some(plan.map(project_agent_plan))),
        };
        notifier.subagent_history_changed(
            task_id,
            &revision.subagent_id,
            revision.revision,
            chat,
            current_plan,
        );
    }
    match project_subagent_catalog(task_id.to_string().into(), Ok(batch.catalog)) {
        Ok(catalog) => notifier.subagent_catalog_changed(catalog),
        Err(error) => crate::logging::warn(
            "subagent_catalog_unpublished",
            serde_json::json!({ "task_id": task_id, "error_code": error.code }),
        ),
    }
}
