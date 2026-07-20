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

pub use model::{
    CommittedArtifactChange, CommittedTaskBatch, TaskProjection, TaskWrite, TerminalOutputAppend,
    ToolArtifactProjection,
};
pub use store::{CommitReceipt, TaskJournalStore};
