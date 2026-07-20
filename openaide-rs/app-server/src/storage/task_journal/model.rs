use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
}

impl TaskWrite {
    /// Creates a Task as a durability barrier; successful return from the
    /// receipt guarantees the complete initial projection survives restart.
    pub fn barrier_create(projection: TaskProjection) -> Self {
        Self {
            task_id: projection.task.task_id.clone(),
            boundary: CommitBoundary::Barrier,
            operations: vec![TaskOperation::Create {
                projection: Box::new(projection),
            }],
            artifacts: Vec::new(),
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(super) enum TaskOperation {
    Create {
        projection: Box<TaskProjection>,
    },
    ReplaceTask {
        task: Box<TaskRecord>,
    },
    AppendText {
        identity: String,
        text: String,
        local_history_updated_at: String,
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
            Self::AppendText { text, .. } => text.len() + 96,
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
    AppendTerminal { terminal_id: String, data: String },
}

impl ArtifactOperation {
    fn estimated_bytes(&self) -> usize {
        match self {
            Self::AppendTerminal { terminal_id, data } => terminal_id.len() + data.len() + 64,
        }
    }
}

/// Lazy normalized Tool-detail state reconstructed only when that Tool expands.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ToolArtifactProjection {
    pub artifact_id: String,
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
    pub artifact_changes: Vec<CommittedArtifactChange>,
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
