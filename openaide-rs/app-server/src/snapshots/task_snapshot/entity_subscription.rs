//! Answers Subagent History and Tool detail subscriptions (ADR-0086): every current field, and
//! the content as `current`, `deltas` after the client's known version, or `full`.
//!
//! A Subagent History is versioned by its revision (ADR-0046) and a Tool detail by its artifact
//! sequence. Both are durable, so a copy cached before an App Server restart resumes the same
//! way as one cached a moment ago.

use std::time::Instant;

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, SubagentId, TaskId};
use openaide_app_server_protocol::snapshot::{
    ChatSnapshot, SubagentHistoryAvailability, SubagentHistorySnapshot,
};
use openaide_app_server_protocol::state::{
    SubagentChatSync, TerminalOutputAppend, ToolTerminalSync,
};
use serde_json::json;

use super::TaskSnapshotStore;
use super::{project_agent_plan, project_chat_item, project_tool_artifact, task_snapshot_error};
use crate::snapshots::{SubagentHistorySubscription, ToolDetailSubscription};

impl TaskSnapshotStore {
    pub(super) fn tool_detail_subscription(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        artifact_id: &str,
        known_version: Option<u64>,
    ) -> Result<ToolDetailSubscription, ProtocolError> {
        let started = Instant::now();
        self.authorize_task(client_instance_id, task_id)?;
        let (artifact, appends) = self
            .store
            .read_tool_artifact_since(task_id.as_str(), artifact_id, known_version)
            .map_err(task_snapshot_error)?;
        let version = artifact.revision;
        let mut details = project_tool_artifact(artifact).ok_or_else(|| ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message: "Tool detail baseline is unavailable".to_string(),
            recoverable: true,
            target: None,
        })?;
        let terminal_sync = match known_version {
            Some(known) if known == version => ToolTerminalSync::Current,
            Some(known) if known < version => ToolTerminalSync::Deltas {
                appends: appends
                    .into_iter()
                    .map(|append| TerminalOutputAppend {
                        terminal_id: append.terminal_id,
                        data: append.data,
                    })
                    .collect(),
            },
            // No version, or one ahead of the artifact: the client cannot be brought current.
            _ => ToolTerminalSync::Full,
        };
        if !matches!(terminal_sync, ToolTerminalSync::Full) {
            details.terminal_outputs.clear();
        }
        let (outcome, append_count) = match &terminal_sync {
            ToolTerminalSync::Current => ("current", 0),
            ToolTerminalSync::Deltas { appends } => ("deltas", appends.len()),
            ToolTerminalSync::Full => ("full", 0),
        };
        crate::logging::info(
            "tool_detail_subscription_resolved",
            json!({
                "operation": "tool_detail_subscribe",
                "task_id": task_id.as_str(),
                "artifact_id": artifact_id,
                "outcome": outcome,
                "known_version": known_version,
                "version": version,
                "append_count": append_count,
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        Ok(ToolDetailSubscription {
            details,
            terminal_sync,
        })
    }

    pub(super) fn subagent_history_subscription(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
        subagent_id: &SubagentId,
        known_version: Option<u64>,
    ) -> Result<SubagentHistorySubscription, ProtocolError> {
        let started = Instant::now();
        self.authorize_task(client_instance_id, task_id)?;
        let catalog = self
            .store
            .subagent_catalog(task_id.as_str())
            .map_err(task_snapshot_error)?;
        let Some(record) = catalog
            .entries
            .iter()
            .find(|entry| entry.subagent_id == subagent_id.as_str())
        else {
            return Err(ProtocolError {
                code: ProtocolErrorCode::NotFound,
                message: "Subagent is unavailable".to_string(),
                recoverable: false,
                target: None,
            });
        };
        let history = self
            .store
            .subagent_history(task_id.as_str(), subagent_id.as_str());
        let (snapshot, chat_sync) = match history {
            Ok(history) => {
                let version = history.revision;
                let availability = if history.messages.is_empty()
                    && matches!(
                        record.status,
                        crate::storage::subagents::SubagentStatusRecord::WaitingForActivity
                    ) {
                    SubagentHistoryAvailability::WaitingForActivity
                } else {
                    SubagentHistoryAvailability::Available
                };
                let current_plan = history.current_plan.map(project_agent_plan);
                if known_version == Some(version) {
                    (
                        SubagentHistorySnapshot {
                            task_id: task_id.clone(),
                            subagent_id: subagent_id.clone(),
                            revision: version,
                            availability,
                            chat: empty_chat(),
                            current_plan,
                            start_cursor: None,
                        },
                        SubagentChatSync::Current,
                    )
                } else {
                    let start = history.messages.len().saturating_sub(self.tail_limit);
                    let retained = &history.messages[start..];
                    let start_cursor = retained
                        .first()
                        .map(|stored| stored.chat.cursor.clone().into());
                    (
                        SubagentHistorySnapshot {
                            task_id: task_id.clone(),
                            subagent_id: subagent_id.clone(),
                            revision: version,
                            availability,
                            chat: ChatSnapshot {
                                items: retained
                                    .iter()
                                    .map(|stored| project_chat_item(&stored.chat))
                                    .collect(),
                                has_more_before: start > 0,
                                has_messages: !history.messages.is_empty(),
                                start_cursor: start_cursor.clone(),
                                end_cursor: retained
                                    .last()
                                    .map(|stored| stored.chat.cursor.clone().into()),
                            },
                            current_plan,
                            start_cursor,
                        },
                        SubagentChatSync::Full,
                    )
                }
            }
            Err(_) => (
                SubagentHistorySnapshot {
                    task_id: task_id.clone(),
                    subagent_id: subagent_id.clone(),
                    revision: record.history_revision,
                    availability: SubagentHistoryAvailability::Unavailable,
                    chat: empty_chat(),
                    current_plan: None,
                    start_cursor: None,
                },
                SubagentChatSync::Full,
            ),
        };
        crate::logging::info(
            "subagent_history_subscription_resolved",
            json!({
                "operation": "subagent_history_subscribe",
                "task_id": task_id.as_str(),
                "subagent_id": subagent_id.as_str(),
                "outcome": match chat_sync {
                    SubagentChatSync::Current => "current",
                    SubagentChatSync::Full => "full",
                },
                "known_version": known_version,
                "version": snapshot.revision,
                "available": snapshot.availability != SubagentHistoryAvailability::Unavailable,
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        Ok(SubagentHistorySubscription {
            history: snapshot,
            chat_sync,
        })
    }

    fn authorize_task(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: &TaskId,
    ) -> Result<(), ProtocolError> {
        let task = self
            .store
            .read_task(task_id.as_str())
            .map_err(task_snapshot_error)?;
        crate::tasks::access::require_client_task_access(&task, client_instance_id)
            .map_err(task_snapshot_error)
    }
}

/// The Chat of a `current` reply: the client keeps its own.
fn empty_chat() -> ChatSnapshot {
    ChatSnapshot {
        items: Vec::new(),
        has_more_before: false,
        has_messages: false,
        start_cursor: None,
        end_cursor: None,
    }
}
