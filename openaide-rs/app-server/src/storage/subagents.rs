use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentPlan, NormalizedMessage};
use crate::storage::cursor;
use crate::storage::records::{MessageMeta, StoredMessage};

use super::Store;

const SUBAGENTS_DIR: &str = "subagents";
const CATALOG_FILE: &str = "catalog.json";
const HISTORY_SNAPSHOT_FILE: &str = "history.snapshot";
const HISTORY_JOURNAL_FILE: &str = "history.journal";
const SCHEMA_VERSION: u16 = 1;
const COMPACT_AFTER_FRAMES: u64 = 64;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryFile {
    schema_version: u16,
    revision: u64,
    journal_frames: u64,
    messages: Vec<StoredMessage>,
    message_meta: MessageMeta,
    current_plan: Option<AgentPlan>,
}

impl Default for HistoryFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            revision: 0,
            journal_frames: 0,
            messages: Vec::new(),
            message_meta: MessageMeta::default(),
            current_plan: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryFrame {
    revision: u64,
    history: HistoryFile,
}

#[derive(Debug, Clone)]
pub struct SubagentHistoryProjection {
    pub revision: u64,
    pub messages: Vec<StoredMessage>,
    pub current_plan: Option<AgentPlan>,
}

impl Store {
    pub fn subagent_catalog(
        &self,
        task_id: &str,
    ) -> Result<SubagentCatalogProjection, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        let catalog = load_catalog(&self.subagent_root(task_id))?;
        Ok(SubagentCatalogProjection {
            revision: catalog.revision,
            entries: catalog.entries,
        })
    }

    pub fn subagent_history(
        &self,
        task_id: &str,
        subagent_id: &str,
    ) -> Result<SubagentHistoryProjection, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        validate_subagent_id(subagent_id)?;
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        let root = self.subagent_root(task_id);
        let catalog = load_catalog(&root)?;
        let record = catalog
            .entries
            .iter()
            .find(|entry| entry.subagent_id == subagent_id)
            .ok_or_else(|| RuntimeError::TaskNotFound("subagent_id".to_string()))?;
        if !record.history_available {
            return Err(RuntimeError::Storage(
                "Subagent history is unavailable".to_string(),
            ));
        }
        let history = load_history(&root.join(subagent_id))?;
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
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        let history = load_history(&self.subagent_root(task_id).join(subagent_id))?;
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
        crate::storage::id::validate_task_id(task_id)?;
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        load_catalog(&self.subagent_root(task_id))?
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
        crate::storage::id::validate_task_id(task_id)?;
        if parent_native_session_id.is_empty() || native_session_id.is_empty() {
            return Err(RuntimeError::InvalidParams(
                "subagent native session id".to_string(),
            ));
        }
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        let root = self.subagent_root(task_id);
        let mut catalog = load_catalog(&root)?;
        if let Some(existing) = catalog
            .entries
            .iter()
            .find(|entry| entry.native_session_id == native_session_id)
        {
            return Ok(existing.clone());
        }
        let parent_subagent_id = catalog
            .entries
            .iter()
            .find(|entry| entry.native_session_id == parent_native_session_id)
            .map(|entry| entry.subagent_id.clone());
        let subagent_id = format!("subagent_{}", uuid::Uuid::new_v4().simple());
        let spawned_order = catalog.next_spawn_order.saturating_add(1);
        catalog.next_spawn_order = spawned_order;
        let record = SubagentRecord {
            subagent_id: subagent_id.clone(),
            native_session_id: native_session_id.to_string(),
            parent_native_session_id: parent_native_session_id.to_string(),
            parent_subagent_id,
            name,
            delegated_task,
            status: SubagentStatusRecord::WaitingForActivity,
            capabilities,
            spawned_order,
            history_revision: 0,
            history_available: true,
            details,
        };
        let history_dir = root.join(&subagent_id);
        fs::create_dir_all(&history_dir)?;
        let mut history = HistoryFile::default();
        history.message_meta.task_id = task_id.to_string();
        crate::storage::atomic::write_json(&history_dir.join(HISTORY_SNAPSHOT_FILE), &history)?;
        durable_create_empty(&history_dir.join(HISTORY_JOURNAL_FILE))?;
        catalog.entries.push(record.clone());
        catalog.revision = catalog.revision.saturating_add(1);
        write_catalog(&root, &catalog)?;
        Ok(record)
    }

    pub fn update_subagent_status(
        &self,
        task_id: &str,
        native_session_id: &str,
        status: SubagentStatusRecord,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.mutate_subagent_record(task_id, native_session_id, |record| {
            record.status = status;
            Ok(())
        })
    }

    pub fn append_subagent_message(
        &self,
        task_id: &str,
        native_session_id: &str,
        mut message: NormalizedMessage,
        upsert: bool,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.mutate_subagent_history(task_id, native_session_id, |history| {
            let identity = message.identity();
            if upsert {
                if let Some(stored) = history
                    .messages
                    .iter_mut()
                    .find(|stored| stored.chat.identity == identity)
                {
                    message.preserve_created_at_from(&stored.chat.message);
                    stored.chat.message = message;
                    advance_history_meta(history);
                    return Ok(());
                }
            }
            let sequence = history
                .messages
                .last()
                .map(|stored| stored.sequence.saturating_add(1))
                .unwrap_or(1);
            history.messages.push(StoredMessage {
                sequence,
                chat: crate::protocol::model::ChatMessage {
                    cursor: cursor::from_sequence(sequence),
                    message_id: identity.clone(),
                    identity,
                    message_type: message.message_type().to_string(),
                    message,
                },
            });
            advance_history_meta(history);
            Ok(())
        })
    }

    pub fn append_subagent_message_part(
        &self,
        task_id: &str,
        native_session_id: &str,
        message: NormalizedMessage,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.mutate_subagent_history(task_id, native_session_id, |history| {
            let identity = message.identity();
            if let Some(stored) = history
                .messages
                .iter_mut()
                .find(|stored| stored.chat.identity == identity)
            {
                let appended = match (&mut stored.chat.message, message) {
                    (
                        NormalizedMessage::AgentMessage { role, parts, .. },
                        NormalizedMessage::AgentMessage {
                            role: incoming_role,
                            parts: incoming_parts,
                            ..
                        },
                    ) if *role == incoming_role && incoming_parts.len() == 1 => {
                        let part = incoming_parts.into_iter().next().expect("one checked part");
                        if let (
                            Some(crate::protocol::model::AgentMessagePart::Text { text }),
                            crate::protocol::model::AgentMessagePart::Text { text: chunk },
                        ) = (parts.last_mut(), &part)
                        {
                            text.push_str(chunk);
                        } else {
                            parts.push(part);
                        }
                        true
                    }
                    (
                        NormalizedMessage::User {
                            text, attachments, ..
                        },
                        NormalizedMessage::User {
                            text: chunk,
                            attachments: incoming_attachments,
                            ..
                        },
                    ) => {
                        text.push_str(&chunk);
                        attachments.extend(incoming_attachments);
                        true
                    }
                    _ => false,
                };
                if !appended {
                    return Err(RuntimeError::Storage(
                        "Subagent message chunk does not match its stored message".to_string(),
                    ));
                }
                advance_history_meta(history);
                return Ok(());
            }
            let sequence = history
                .messages
                .last()
                .map(|stored| stored.sequence.saturating_add(1))
                .unwrap_or(1);
            history.messages.push(StoredMessage {
                sequence,
                chat: crate::protocol::model::ChatMessage {
                    cursor: cursor::from_sequence(sequence),
                    message_id: identity.clone(),
                    identity,
                    message_type: message.message_type().to_string(),
                    message,
                },
            });
            advance_history_meta(history);
            Ok(())
        })
    }

    pub fn set_subagent_plan(
        &self,
        task_id: &str,
        native_session_id: &str,
        plan: AgentPlan,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.mutate_subagent_history(task_id, native_session_id, |history| {
            history.current_plan = (!plan.entries.is_empty()).then_some(plan);
            Ok(())
        })
    }

    pub fn record_subagent_permission_outcome(
        &self,
        task_id: &str,
        native_session_id: &str,
        tool_call_id: &str,
        outcome: crate::protocol::model::ToolPermissionOutcome,
    ) -> Result<SubagentRecord, RuntimeError> {
        self.mutate_subagent_history(task_id, native_session_id, |history| {
            let Some(outcomes) = history.messages.iter_mut().find_map(|stored| {
                let NormalizedMessage::Activity { steps, .. } = &mut stored.chat.message else {
                    return None;
                };
                steps.iter_mut().find_map(|step| match step {
                    crate::protocol::model::ActivityStep::Tool {
                        tool_call_id: Some(id),
                        permission_outcomes,
                        ..
                    } if id == tool_call_id => Some(permission_outcomes),
                    _ => None,
                })
            }) else {
                return Err(RuntimeError::Storage(
                    "Subagent permission outcome has no matching tool call".to_string(),
                ));
            };
            if let Some(existing) = outcomes
                .iter_mut()
                .find(|existing| existing.request_id == outcome.request_id)
            {
                *existing = outcome;
            } else {
                outcomes.push(outcome);
            }
            advance_history_meta(history);
            Ok(())
        })
    }

    fn mutate_subagent_record(
        &self,
        task_id: &str,
        native_session_id: &str,
        mutate: impl FnOnce(&mut SubagentRecord) -> Result<(), RuntimeError>,
    ) -> Result<SubagentRecord, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        let root = self.subagent_root(task_id);
        let mut catalog = load_catalog(&root)?;
        let record = catalog
            .entries
            .iter_mut()
            .find(|entry| entry.native_session_id == native_session_id)
            .ok_or_else(|| RuntimeError::TaskNotFound("subagent native session".to_string()))?;
        mutate(record)?;
        let result = record.clone();
        catalog.revision = catalog.revision.saturating_add(1);
        write_catalog(&root, &catalog)?;
        Ok(result)
    }

    fn mutate_subagent_history(
        &self,
        task_id: &str,
        native_session_id: &str,
        mutate: impl FnOnce(&mut HistoryFile) -> Result<(), RuntimeError>,
    ) -> Result<SubagentRecord, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        let _guard = self
            .inner
            .subagent_write_lock
            .lock()
            .expect("Subagent store lock poisoned");
        let root = self.subagent_root(task_id);
        let mut catalog = load_catalog(&root)?;
        let record = catalog
            .entries
            .iter_mut()
            .find(|entry| entry.native_session_id == native_session_id)
            .ok_or_else(|| RuntimeError::TaskNotFound("subagent native session".to_string()))?;
        let history_dir = root.join(&record.subagent_id);
        let mut history = match load_history(&history_dir) {
            Ok(history) => history,
            Err(error) => {
                record.history_available = false;
                catalog.revision = catalog.revision.saturating_add(1);
                write_catalog(&root, &catalog)?;
                return Err(error);
            }
        };
        mutate(&mut history)?;
        history.revision = history.revision.saturating_add(1);
        history.journal_frames = history.journal_frames.saturating_add(1);
        append_history_frame(&history_dir.join(HISTORY_JOURNAL_FILE), &history)?;
        if history.journal_frames >= COMPACT_AFTER_FRAMES {
            history.journal_frames = 0;
            crate::storage::atomic::write_json(&history_dir.join(HISTORY_SNAPSHOT_FILE), &history)?;
            durable_create_empty(&history_dir.join(HISTORY_JOURNAL_FILE))?;
        }
        record.history_revision = history.revision;
        record.history_available = true;
        if record.status == SubagentStatusRecord::WaitingForActivity {
            record.status = SubagentStatusRecord::Running;
        }
        let result = record.clone();
        catalog.revision = catalog.revision.saturating_add(1);
        write_catalog(&root, &catalog)?;
        Ok(result)
    }

    fn subagent_root(&self, task_id: &str) -> PathBuf {
        self.root()
            .join("task-store-v1")
            .join("tasks")
            .join(task_id)
            .join(SUBAGENTS_DIR)
    }
}

fn load_catalog(root: &Path) -> Result<CatalogFile, RuntimeError> {
    let path = root.join(CATALOG_FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CatalogFile {
                schema_version: SCHEMA_VERSION,
                ..Default::default()
            })
        }
        Err(error) => return Err(error.into()),
    };
    let catalog: CatalogFile = serde_json::from_slice(&bytes)?;
    validate_schema(catalog.schema_version)?;
    Ok(catalog)
}

fn write_catalog(root: &Path, catalog: &CatalogFile) -> Result<(), RuntimeError> {
    crate::storage::atomic::write_json(&root.join(CATALOG_FILE), catalog)
}

fn load_history(dir: &Path) -> Result<HistoryFile, RuntimeError> {
    let bytes = fs::read(dir.join(HISTORY_SNAPSHOT_FILE))?;
    let mut history: HistoryFile = serde_json::from_slice(&bytes)?;
    validate_schema(history.schema_version)?;
    let journal = dir.join(HISTORY_JOURNAL_FILE);
    let file = fs::File::open(journal)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let frame: HistoryFrame = serde_json::from_str(&line)?;
        if frame.revision != history.revision.saturating_add(1) {
            return Err(RuntimeError::Storage(
                "Subagent history journal sequence is invalid".to_string(),
            ));
        }
        history = frame.history;
    }
    // codex-acp previously projected the encrypted causal-root placeholder as if it
    // were child-authored User text. The identity is adapter-owned, so suppressing
    // that exact legacy shape does not discard genuine provider-neutral history.
    history.messages.retain(|stored| {
        let identity = stored.chat.identity.as_str();
        !(identity.contains(":user:collab:") && identity.ends_with(":prompt"))
    });
    for stored in &mut history.messages {
        if !stored.chat.identity.contains(":codex:") {
            continue;
        }
        if let NormalizedMessage::Activity { steps, .. } = &mut stored.chat.message {
            if let [crate::protocol::model::ActivityStep::Text { level, .. }] = steps.as_mut_slice()
            {
                *level = Some("agent_boundary".to_string());
            }
        }
    }
    Ok(history)
}

fn append_history_frame(path: &Path, history: &HistoryFile) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec(&HistoryFrame {
        revision: history.revision,
        history: history.clone(),
    })?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all().ok();
    Ok(())
}

fn advance_history_meta(history: &mut HistoryFile) {
    let previous = history
        .message_meta
        .local_history_updated_at
        .parse::<u128>()
        .unwrap_or_default();
    let now = crate::time::now_string()
        .parse::<u128>()
        .unwrap_or_default();
    history.message_meta.version = history.message_meta.version.saturating_add(1);
    history.message_meta.message_count = history.messages.len() as u64;
    history.message_meta.local_history_updated_at = now.max(previous.saturating_add(1)).to_string();
    history.message_meta.first_cursor = history
        .messages
        .first()
        .map(|message| message.chat.cursor.clone());
    history.message_meta.last_cursor = history
        .messages
        .last()
        .map(|message| message.chat.cursor.clone());
}

fn durable_create_empty(path: &Path) -> Result<(), RuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Subagent path has no parent".to_string()))?;
    fs::create_dir_all(parent)?;
    let file = fs::File::create(path)?;
    file.sync_all().ok();
    Ok(())
}

fn validate_schema(version: u16) -> Result<(), RuntimeError> {
    if version == SCHEMA_VERSION {
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
