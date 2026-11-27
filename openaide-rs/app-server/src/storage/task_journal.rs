//! Durable Task metadata plus lazy snapshot-and-delta Chat persistence.
//!
//! Callers submit normalized Task writes and observe only durable commit results.
//! Framing, lazy migration, batching, artifact visibility, and compaction stay
//! behind this interface so workflow code never depends on physical files.

mod artifact;
mod catalog;
mod chat_log;
/// Shared with the Subagent Chat streams the worker writes (ADR-0076).
pub(crate) mod frame;
mod generation_layout;
mod images;
mod model;
mod newer_chat;
mod projection;
mod scheduler;
mod split;
mod store;

#[allow(unused_imports)] // TODO(state-sync phase 3): the Task subscription reads these.
pub(crate) use chat_log::{ChatChangesSince, ChatFrameChanges};
pub(crate) use model::TaskOperation;
pub use model::{
    CommittedArtifactChange, CommittedChatTextAppend, CommittedTaskBatch, TaskJournalQueueMetrics,
    TaskProjection, TaskStorageFailure, TaskStorageFatalFailure, TaskWrite, TerminalOutputAppend,
    ToolArtifactProjection, ToolArtifactReplacement, ToolTerminalAppend,
};
pub(crate) use store::TrySubmit;
pub use store::{CommitReceipt, TaskJournalStore};
