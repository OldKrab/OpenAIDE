use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::protocol::model::ActivityToolDetails;
use crate::storage::records::{MessageMeta, StoredMessage, TaskRecord};

/// Complete normalized Task state reconstructed by journal replay.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskProjection {
    pub task: TaskRecord,
    pub messages: Vec<StoredMessage>,
    pub message_meta: MessageMeta,
    /// Highest artifact operation made visible by a durable Task commit.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub artifact_heads: HashMap<String, u64>,
}

/// One admitted semantic write. Constructors preserve the ordering boundary at
/// the interface instead of asking callers to understand worker commands.
#[derive(Debug)]
pub struct TaskWrite {
    pub(super) task_id: String,
    pub(super) boundary: CommitBoundary,
    pub(super) operations: Vec<TaskOperation>,
    pub(super) artifacts: Vec<ArtifactWrite>,
    pub(super) compaction: CompactionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompactionMode {
    None,
    IfWorthwhile,
    Force,
}

impl TaskWrite {
    /// Creates a Task as a durability barrier; successful return from the
    /// receipt guarantees the complete initial projection survives restart.
    pub fn barrier_create(projection: TaskProjection) -> Self {
        Self::barrier_create_with_artifacts(projection, Vec::new())
    }

    /// Creates a Task and its initial lazy Tool details in one visibility commit.
    pub fn barrier_create_with_artifacts(
        projection: TaskProjection,
        artifacts: Vec<ToolArtifactReplacement>,
    ) -> Self {
        Self {
            task_id: projection.task.task_id.clone(),
            boundary: CommitBoundary::Barrier,
            operations: vec![TaskOperation::Create {
                projection: Box::new(projection),
            }],
            artifacts: replacement_writes(artifacts),
            compaction: CompactionMode::None,
        }
    }

    /// Replaces the small durable Task record without rewriting Chat or Tool
    /// artifacts. The receipt is a workflow durability barrier.
    pub fn barrier_replace_task(task: TaskRecord) -> Self {
        Self {
            task_id: task.task_id.clone(),
            boundary: CommitBoundary::Barrier,
            operations: vec![TaskOperation::ReplaceTask {
                task: Box::new(task),
            }],
            artifacts: Vec::new(),
            compaction: CompactionMode::None,
        }
    }

    /// Atomically replaces the normalized Task/Chat projection and any changed
    /// lazy Tool details. This is the workflow transaction boundary.
    pub fn barrier_replace_projection(
        projection: TaskProjection,
        artifacts: Vec<ToolArtifactReplacement>,
    ) -> Self {
        Self::barrier_replace_projection_with_terminal(projection, artifacts, Vec::new())
    }

    /// Commits structured and streamed Tool changes with the same Task draft.
    pub fn barrier_replace_projection_with_terminal(
        projection: TaskProjection,
        artifacts: Vec<ToolArtifactReplacement>,
        terminal_appends: Vec<ToolTerminalAppend>,
    ) -> Self {
        let mut writes = replacement_writes(artifacts);
        writes.extend(terminal_writes(terminal_appends));
        Self {
            task_id: projection.task.task_id.clone(),
            boundary: CommitBoundary::Barrier,
            operations: vec![TaskOperation::ReplaceProjection {
                projection: Box::new(projection),
            }],
            artifacts: writes,
            compaction: CompactionMode::None,
        }
    }

    /// Commits one normalized Task/Chat transaction without serializing unchanged history.
    pub(crate) fn barrier_operations_with_artifacts(
        task_id: String,
        operations: Vec<TaskOperation>,
        artifacts: Vec<ToolArtifactReplacement>,
        terminal_appends: Vec<ToolTerminalAppend>,
    ) -> Self {
        let mut writes = replacement_writes(artifacts);
        writes.extend(terminal_writes(terminal_appends));
        Self {
            task_id,
            boundary: CommitBoundary::Barrier,
            operations,
            artifacts: writes,
            compaction: CompactionMode::None,
        }
    }

    /// Admits an ordered group from one ACP wire update without revising Task.
    pub fn stream_append_terminals(
        task_id: impl Into<String>,
        terminal_appends: Vec<ToolTerminalAppend>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            boundary: CommitBoundary::Stream,
            operations: Vec::new(),
            artifacts: terminal_writes(terminal_appends),
            compaction: CompactionMode::None,
        }
    }

    /// Admits one streamed text delta without forcing the caller to wait for
    /// disk. Adjacent deltas for the same message are combined by the worker.
    pub fn stream_append_text(
        task_id: impl Into<String>,
        identity: impl Into<String>,
        text: impl Into<String>,
        local_history_updated_at: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            boundary: CommitBoundary::Stream,
            operations: vec![TaskOperation::AppendText {
                identity: identity.into(),
                text: text.into(),
                local_history_updated_at: local_history_updated_at.into(),
            }],
            artifacts: Vec::new(),
            compaction: CompactionMode::None,
        }
    }

    /// Admits Agent-owned terminal bytes for a lazy Tool artifact. The worker
    /// prepares the artifact frame before committing its Task visibility head.
    pub fn stream_append_terminal(
        task_id: impl Into<String>,
        artifact_id: impl Into<String>,
        terminal_id: impl Into<String>,
        data: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            boundary: CommitBoundary::Stream,
            operations: Vec::new(),
            artifacts: vec![ArtifactWrite {
                artifact_id: artifact_id.into(),
                operation: ArtifactOperation::AppendTerminal {
                    terminal_id: terminal_id.into(),
                    data: data.into(),
                },
            }],
            compaction: CompactionMode::None,
        }
    }

    /// Seals every earlier operation for one Task without creating a journal
    /// frame when there is no pending state change.
    pub fn barrier(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            boundary: CommitBoundary::Barrier,
            operations: Vec::new(),
            artifacts: Vec::new(),
            compaction: CompactionMode::None,
        }
    }

    /// Seals preceding writes and replaces the Task journal with one verified
    /// canonical projection frame.
    pub fn compaction_barrier(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            boundary: CommitBoundary::Barrier,
            operations: Vec::new(),
            artifacts: Vec::new(),
            compaction: CompactionMode::Force,
        }
    }

    /// Checks measured frame/byte thresholds at an idle prompt boundary and
    /// compacts only when the canonical projection would reclaim useful space.
    pub fn compaction_if_worthwhile_barrier(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            boundary: CommitBoundary::Barrier,
            operations: Vec::new(),
            artifacts: Vec::new(),
            compaction: CompactionMode::IfWorthwhile,
        }
    }

    pub(super) fn estimated_bytes(&self) -> usize {
        self.operations
            .iter()
            .map(TaskOperation::estimated_bytes)
            .sum::<usize>()
            .saturating_add(
                self.artifacts
                    .iter()
                    .map(ArtifactWrite::estimated_bytes)
                    .sum(),
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitBoundary {
    Stream,
    Barrier,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(crate) enum TaskOperation {
    Create {
        projection: Box<TaskProjection>,
    },
    ReplaceTask {
        task: Box<TaskRecord>,
    },
    ReplaceProjection {
        projection: Box<TaskProjection>,
    },
    AppendText {
        identity: String,
        text: String,
        local_history_updated_at: String,
    },
    AppendMessage {
        message: Box<StoredMessage>,
    },
    UpsertMessage {
        message: Box<StoredMessage>,
    },
    ReplaceMessages {
        messages: Vec<StoredMessage>,
        message_meta: Box<MessageMeta>,
    },
    ReplaceMessageMeta {
        message_meta: Box<MessageMeta>,
    },
    CommitArtifact {
        artifact_id: String,
        artifact_sequence: u64,
    },
}

impl TaskOperation {
    fn estimated_bytes(&self) -> usize {
        match self {
            Self::Create { projection } => projection.messages.len().saturating_mul(128) + 2_048,
            Self::ReplaceTask { .. } => 2_048,
            Self::ReplaceProjection { projection } => {
                serde_json::to_vec(projection).map_or(usize::MAX, |bytes| bytes.len())
            }
            Self::AppendText { text, .. } => text.len() + 96,
            Self::AppendMessage { message } | Self::UpsertMessage { message } => {
                serde_json::to_vec(message).map_or(usize::MAX, |bytes| bytes.len())
            }
            Self::ReplaceMessages { messages, .. } => {
                serde_json::to_vec(messages).map_or(usize::MAX, |bytes| bytes.len())
            }
            Self::ReplaceMessageMeta { .. } => 256,
            Self::CommitArtifact { artifact_id, .. } => artifact_id.len() + 64,
        }
    }
}

#[derive(Debug)]
pub(super) struct ArtifactWrite {
    pub artifact_id: String,
    pub operation: ArtifactOperation,
}

impl ArtifactWrite {
    fn estimated_bytes(&self) -> usize {
        self.artifact_id.len() + self.operation.estimated_bytes()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(super) enum ArtifactOperation {
    ReplaceDetails { details: Box<ActivityToolDetails> },
    AppendTerminal { terminal_id: String, data: String },
}

impl ArtifactOperation {
    fn estimated_bytes(&self) -> usize {
        match self {
            Self::ReplaceDetails { details } => {
                serde_json::to_vec(details).map_or(usize::MAX, |bytes| bytes.len())
            }
            Self::AppendTerminal { terminal_id, data } => terminal_id.len() + data.len() + 64,
        }
    }
}

/// Structured Tool detail staged with a Task workflow transaction.
#[derive(Debug, Clone)]
pub struct ToolArtifactReplacement {
    pub artifact_id: String,
    pub details: ActivityToolDetails,
}

#[derive(Debug, Clone)]
pub struct ToolTerminalAppend {
    pub artifact_id: String,
    pub terminal_id: String,
    pub data: String,
}

fn replacement_writes(replacements: Vec<ToolArtifactReplacement>) -> Vec<ArtifactWrite> {
    replacements
        .into_iter()
        .map(|replacement| ArtifactWrite {
            artifact_id: replacement.artifact_id,
            operation: ArtifactOperation::ReplaceDetails {
                details: Box::new(replacement.details),
            },
        })
        .collect()
}

fn terminal_writes(appends: Vec<ToolTerminalAppend>) -> Vec<ArtifactWrite> {
    appends
        .into_iter()
        .map(|append| ArtifactWrite {
            artifact_id: append.artifact_id,
            operation: ArtifactOperation::AppendTerminal {
                terminal_id: append.terminal_id,
                data: append.data,
            },
        })
        .collect()
}

/// Lazy normalized Tool-detail state reconstructed only when that Tool expands.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ToolArtifactProjection {
    pub artifact_id: String,
    /// Durable Task-referenced artifact head; never sourced from uncommitted artifact bytes.
    #[serde(default, skip_serializing)]
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<ActivityToolDetails>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminal_order: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub terminal_outputs: HashMap<String, String>,
}

/// Exact durable result emitted after the journal sync completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedTaskBatch {
    pub task_id: String,
    pub journal_sequence: u64,
    /// True only when the durable batch changes the public Task snapshot.
    pub task_snapshot_changed: bool,
    /// Artifacts whose structured details were replaced by this batch. The
    /// synchronous Task publisher owns their complete same-revision delta.
    pub replaced_artifact_ids: Vec<String>,
    pub artifact_changes: Vec<CommittedArtifactChange>,
}

/// Path-free notification that a Task was frozen after a physical write failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStorageFailure {
    pub task_id: String,
}

/// Path-free root-wide signal emitted when the sole storage worker dies.
/// The App Server process supervisor owns this signal and terminates the
/// process instead of continuing with an invalid durability authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStorageFatalFailure {
    pub reason: &'static str,
}

/// Retained payload high-water marks for the bounded stream scheduler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TaskJournalQueueMetrics {
    pub peak_global_stream_bytes: usize,
    pub peak_task_stream_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedArtifactChange {
    pub artifact_id: String,
    pub artifact_sequence: u64,
    pub terminal_appends: Vec<TerminalOutputAppend>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalOutputAppend {
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct JournalFrame {
    pub format_version: u16,
    pub sequence: u64,
    pub operations: Vec<TaskOperation>,
}

impl super::frame::FramedRecord for JournalFrame {
    fn format_version(&self) -> u16 {
        self.format_version
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct ArtifactFrame {
    pub format_version: u16,
    pub sequence: u64,
    pub operations: Vec<ArtifactOperation>,
}

impl super::frame::FramedRecord for ArtifactFrame {
    fn format_version(&self) -> u16 {
        self.format_version
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }
}
