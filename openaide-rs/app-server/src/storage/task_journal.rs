//! Canonical journal-backed persistence for complete Task state.
//!
//! Callers submit normalized Task writes and observe only durable commit results.
//! Framing, replay, batching, artifact visibility, and compaction stay behind this
//! interface so workflow code never depends on physical files.

mod artifact;
mod frame;
mod model;
mod projection;
mod scheduler;
mod store;

pub(crate) use model::TaskOperation;
pub use model::{
    CommittedArtifactChange, CommittedTaskBatch, TaskJournalQueueMetrics, TaskProjection,
    TaskStorageFailure, TaskStorageFatalFailure, TaskWrite, TerminalOutputAppend,
    ToolArtifactProjection, ToolArtifactReplacement, ToolTerminalAppend,
};
pub(crate) use store::TrySubmit;
pub use store::{CommitReceipt, TaskJournalStore};
