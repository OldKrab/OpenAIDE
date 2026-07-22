//! Durable Task metadata plus lazy snapshot-and-delta Chat persistence.
//!
//! Callers submit normalized Task writes and observe only durable commit results.
//! Framing, lazy migration, batching, artifact visibility, and compaction stay
//! behind this interface so workflow code never depends on physical files.

mod artifact;
mod catalog;
mod frame;
mod model;
mod projection;
mod scheduler;
mod split;
mod store;

pub(crate) use model::TaskOperation;
pub use model::{
    CommittedArtifactChange, CommittedTaskBatch, TaskJournalQueueMetrics, TaskProjection,
    TaskStorageFailure, TaskStorageFatalFailure, TaskWrite, TerminalOutputAppend,
    ToolArtifactProjection, ToolArtifactReplacement, ToolTerminalAppend,
};
pub(crate) use store::TrySubmit;
pub use store::{CommitReceipt, TaskJournalStore};
