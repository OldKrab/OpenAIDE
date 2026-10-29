//! Subagent catalogs and histories of a Task (ADR-0046). The Task journal worker
//! writes them in the Task's queue order, each history as a Chat stream
//! (ADR-0076): a streamed change is a write the caller does not wait for, and a
//! lifecycle change is a barrier.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentPlan, ChatMessage, NormalizedMessage, ToolPermissionOutcome};
use crate::storage::records::StoredMessage;
use crate::storage::task_journal::{CommitReceipt, TaskWrite};

use super::Store;

mod legacy;
mod state;
mod stream;
mod writes;
pub(crate) use state::SubagentState;
pub use state::{CommittedSubagentBatch, CommittedSubagentRevision};
use stream::{HistoryChange, SubagentChat, STREAM_HEAD_FILE};
pub(crate) use writes::{SubagentOperation, SubagentWrite};

const SUBAGENTS_DIR: &str = "subagents";
const CATALOG_FILE: &str = "catalog.json";
/// Version 2 marks a Task whose histories may be Chat streams, so an App Server
/// that reads only ADR-0046 histories rejects the catalog without changing it.
const CATALOG_SCHEMA_VERSION: u16 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFile {
    schema_version: u16,
    revision: u64,
    next_spawn_order: u64,
    entries: Vec<SubagentRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentRecord {
    pub subagent_id: String,
    pub native_session_id: String,
    pub parent_native_session_id: String,
    pub parent_subagent_id: Option<String>,
    pub name: String,
    pub delegated_task: String,
    pub status: SubagentStatusRecord,
    pub capabilities: SubagentCapabilitiesRecord,
    pub spawned_order: u64,
    pub history_revision: u64,
    pub history_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<SubagentDetailRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubagentStatusRecord {
    WaitingForActivity,
    Running,
    Completed,
    Failed,
    Cancelled,
    Disconnected,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCapabilitiesRecord {
    pub cancel: bool,
    pub close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDetailRecord {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct SubagentCatalogProjection {
    pub revision: u64,
    pub entries: Vec<SubagentRecord>,
}

#[derive(Debug, Clone)]
pub struct SubagentHistoryProjection {
    pub revision: u64,
    pub messages: Vec<StoredMessage>,
    pub current_plan: Option<AgentPlan>,
}

#[derive(Debug, Clone)]
pub enum CommittedSubagentChange {
    /// A message added after every existing message.
    Append(ChatMessage),
    /// A complete replacement of the message with the same identity.
    Upsert(ChatMessage),
    /// Text added to the last text part of the message.
    AppendText { message_id: String, text: String },
    /// The whole current plan; `None` clears it.
    Plan(Option<AgentPlan>),
}

impl Store {
    pub fn subagent_catalog(
        &self,
        task_id: &str,
    ) -> Result<SubagentCatalogProjection, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        self.subagent_state()
            .catalog(&self.subagent_root(task_id), task_id)
    }

    pub fn subagent_history(
        &self,
        task_id: &str,
        subagent_id: &str,
    ) -> Result<SubagentHistoryProjection, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        validate_subagent_id(subagent_id)?;
        let history = self.subagent_state().history(
            &self.subagent_root(task_id),
            task_id,
            subagent_id,
            true,
        )?;
        Ok(SubagentHistoryProjection {
            revision: history.revision,
            messages: history.messages,
            current_plan: history.current_plan,
        })
    }

    pub fn subagent_page_before(
        &self,
        task_id: &str,
        subagent_id: &str,
        before_cursor: &str,
        limit: usize,
    ) -> Result<crate::protocol::model::MessagePage, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        validate_subagent_id(subagent_id)?;
        let history = self.subagent_state().history(
            &self.subagent_root(task_id),
            task_id,
            subagent_id,
            false,
        )?;
        let end = history
            .messages
            .iter()
            .position(|message| message.chat.cursor == before_cursor)
            .ok_or_else(|| RuntimeError::InvalidParams("before_cursor".to_string()))?;
        let start = crate::storage::message_store::chat_page_start(
            &history.messages,
            end.saturating_sub(limit.clamp(1, 500)),
            end,
        );
        let items = history.messages[start..end]
            .iter()
            .map(|stored| stored.chat.clone())
            .collect::<Vec<_>>();
        Ok(crate::protocol::model::MessagePage {
            task_id: task_id.to_string(),
            start_cursor: items.first().map(|message| message.cursor.clone()),
            end_cursor: items.last().map(|message| message.cursor.clone()),
            items,
            has_before: start > 0,
            total_count: history.messages.len() as u64,
            version: history.revision,
        })
    }

    pub fn subagent_record_by_native(
        &self,
        task_id: &str,
        native_session_id: &str,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.subagent_catalog(task_id)?
            .entries
            .into_iter()
            .find(|entry| entry.native_session_id == native_session_id)
            .ok_or_else(|| RuntimeError::TaskNotFound("subagent native session".to_string()))
    }

    /// Reconciles one Agent-announced lifecycle generation. Native session ids stay
    /// inside this storage seam and are never projected into App Server Protocol.
    #[allow(clippy::too_many_arguments)]
    pub fn record_subagent_spawn(
        &self,
        task_id: &str,
        parent_native_session_id: &str,
        native_session_id: &str,
        name: String,
        delegated_task: String,
        capabilities: SubagentCapabilitiesRecord,
        details: Vec<SubagentDetailRecord>,
    ) -> Result<SubagentRecord, RuntimeError> {
        if parent_native_session_id.is_empty() || native_session_id.is_empty() {
            return Err(RuntimeError::InvalidParams(
                "subagent native session id".to_string(),
            ));
        }
        self.commit_subagent_write(
            task_id,
            native_session_id,
            SubagentOperation::Spawn {
                parent_native_session_id: parent_native_session_id.to_string(),
                name,
                delegated_task,
                capabilities,
                details,
            },
        )
    }

    pub fn update_subagent_status(
        &self,
        task_id: &str,
        native_session_id: &str,
        status: SubagentStatusRecord,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.commit_subagent_write(
            task_id,
            native_session_id,
            SubagentOperation::SetStatus(status),
        )
    }

    /// Applies an Agent-reported identity patch. `None` leaves a field as it is.
    pub fn update_subagent_identity(
        &self,
        task_id: &str,
        native_session_id: &str,
        name: Option<String>,
        delegated_task: Option<String>,
        cancel: Option<bool>,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.commit_subagent_write(
            task_id,
            native_session_id,
            SubagentOperation::UpdateIdentity {
                name,
                delegated_task,
                cancel,
            },
        )
    }

    /// Replaces stored details that share a label with an incoming one and
    /// appends the rest, so a later Agent report refines the announced value.
    pub fn merge_subagent_details(
        &self,
        task_id: &str,
        native_session_id: &str,
        details: Vec<SubagentDetailRecord>,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.commit_subagent_write(
            task_id,
            native_session_id,
            SubagentOperation::MergeDetails(details),
        )
    }

    /// Queues a message, or with `upsert` a replacement of the message with its
    /// identity. Returns once the write is queued; publication follows its commit.
    pub fn append_subagent_message(
        &self,
        task_id: &str,
        native_session_id: &str,
        message: NormalizedMessage,
        upsert: bool,
    ) -> Result<CommitReceipt, RuntimeError> {
        self.stream_subagent_write(
            task_id,
            native_session_id,
            SubagentOperation::AppendMessage { message, upsert },
        )
    }

    /// Queues one streamed chunk of the message with its identity.
    pub fn append_subagent_message_part(
        &self,
        task_id: &str,
        native_session_id: &str,
        message: NormalizedMessage,
    ) -> Result<CommitReceipt, RuntimeError> {
        self.stream_subagent_write(
            task_id,
            native_session_id,
            SubagentOperation::AppendMessagePart(message),
        )
    }

    pub fn set_subagent_plan(
        &self,
        task_id: &str,
        native_session_id: &str,
        plan: AgentPlan,
    ) -> Result<CommitReceipt, RuntimeError> {
        self.stream_subagent_write(task_id, native_session_id, SubagentOperation::SetPlan(plan))
    }

    /// Records a permission decision as a barrier, so its caller learns whether
    /// it was saved.
    pub fn record_subagent_permission_outcome(
        &self,
        task_id: &str,
        native_session_id: &str,
        tool_call_id: &str,
        outcome: ToolPermissionOutcome,
    ) -> Result<(), RuntimeError> {
        self.commit_subagent_write(
            task_id,
            native_session_id,
            SubagentOperation::RecordPermissionOutcome {
                tool_call_id: tool_call_id.to_string(),
                outcome,
            },
        )
        .map(|_| ())
    }

    /// Commits one write after every earlier write of the Task and returns the
    /// Subagent's committed record.
    fn commit_subagent_write(
        &self,
        task_id: &str,
        native_session_id: &str,
        operation: SubagentOperation,
    ) -> Result<SubagentRecord, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        let write = SubagentWrite {
            native_session_id: native_session_id.to_string(),
            operation,
        };
        self.inner
            .task_journal
            .submit(TaskWrite::barrier_subagent(task_id.to_string(), write))?
            .wait()?;
        self.subagent_record_by_native(task_id, native_session_id)
    }

    /// Queues one write without waiting for its commit; dropping the receipt
    /// leaves the write to the worker, which logs its failure.
    fn stream_subagent_write(
        &self,
        task_id: &str,
        native_session_id: &str,
        operation: SubagentOperation,
    ) -> Result<CommitReceipt, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        let write = SubagentWrite {
            native_session_id: native_session_id.to_string(),
            operation,
        };
        self.inner
            .task_journal
            .submit(TaskWrite::stream_subagent(task_id.to_string(), write))
    }

    fn subagent_state(&self) -> &SubagentState {
        self.inner.task_journal.subagents()
    }

    fn subagent_root(&self, task_id: &str) -> PathBuf {
        self.root()
            .join("task-store-v1")
            .join("tasks")
            .join(task_id)
            .join(SUBAGENTS_DIR)
    }
}

/// Describes what `change` did to `history`, which held `message_count` messages
/// before it.
fn committed_change(
    history: &SubagentChat,
    change: &HistoryChange,
    message_count: usize,
) -> Option<CommittedSubagentChange> {
    let find = |identity: &str| {
        history
            .messages
            .iter()
            .find(|stored| stored.chat.identity == identity)
            .map(|stored| &stored.chat)
    };
    Some(match change {
        HistoryChange::AppendText { identity, text } => CommittedSubagentChange::AppendText {
            message_id: find(identity)?.message_id.clone(),
            text: text.clone(),
        },
        HistoryChange::Upsert(identity) => {
            let message = find(identity)?.clone();
            if history.messages.len() > message_count {
                CommittedSubagentChange::Append(message)
            } else {
                CommittedSubagentChange::Upsert(message)
            }
        }
        HistoryChange::Plan => CommittedSubagentChange::Plan(history.current_plan.clone()),
    })
}

/// Returns which of `image_ids` any Subagent history of the Task still names, so
/// the Chat compaction sweep keeps those image files (ADR-0075). It scans the
/// stored bytes of every history file, in either format, instead of parsing
/// them, and runs only for old files Chat no longer references.
pub(crate) fn subagent_image_references(
    task_dir: &Path,
    image_ids: &[String],
) -> Result<std::collections::HashSet<String>, RuntimeError> {
    let mut found = std::collections::HashSet::new();
    let entries = match fs::read_dir(task_dir.join(SUBAGENTS_DIR)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        for file in fs::read_dir(entry.path())? {
            let file = file?;
            if file.file_name() == STREAM_HEAD_FILE || !file.file_type()?.is_file() {
                continue;
            }
            match fs::read(file.path()) {
                Ok(bytes) => {
                    crate::storage::chat_images::ids_mentioned_in(&bytes, image_ids, &mut found)
                }
                // A concurrent compaction removed an old generation.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(found)
}

fn load_catalog(root: &Path) -> Result<CatalogFile, RuntimeError> {
    let path = root.join(CATALOG_FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CatalogFile {
                schema_version: CATALOG_SCHEMA_VERSION,
                ..Default::default()
            })
        }
        Err(error) => return Err(error.into()),
    };
    let catalog: CatalogFile = serde_json::from_slice(&bytes)?;
    validate_schema(catalog.schema_version)?;
    Ok(catalog)
}

/// Writes the catalog at the current schema version, which it then holds.
fn write_catalog(root: &Path, catalog: &mut CatalogFile) -> Result<(), RuntimeError> {
    let previous = std::mem::replace(&mut catalog.schema_version, CATALOG_SCHEMA_VERSION);
    let written = crate::storage::atomic::write_json(&root.join(CATALOG_FILE), catalog);
    if written.is_err() {
        catalog.schema_version = previous;
    }
    written
}

fn validate_schema(version: u16) -> Result<(), RuntimeError> {
    if (1..=CATALOG_SCHEMA_VERSION).contains(&version) {
        Ok(())
    } else {
        Err(RuntimeError::Storage(
            "Unsupported Subagent history schema".to_string(),
        ))
    }
}

fn validate_subagent_id(value: &str) -> Result<(), RuntimeError> {
    let valid = value.strip_prefix("subagent_").is_some_and(|suffix| {
        suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    if valid {
        Ok(())
    } else {
        Err(RuntimeError::InvalidParams("subagent_id".to_string()))
    }
}

#[cfg(test)]
#[path = "subagents_tests.rs"]
mod tests;
