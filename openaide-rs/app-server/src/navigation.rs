//! Task Navigation as a server-computed view subscription (ADR-0086, design 2.2).
//!
//! One serialized [`NavigationOwner`] serves every client's views. Its inputs are Project dirty
//! marks, Project or Agent collection changes, and view parameter changes. It recomputes the
//! dirty Projects' sorted rows once per flush, then diffs each view against what it last sent
//! that client. Because the owner runs under the protocol lock, no handler publishes
//! Navigation itself: handlers mark dirty and the flush emits the frames ahead of their reply.

mod dirty;
mod owner;
mod rows;
mod view;

pub(crate) use dirty::NavigationDirty;
pub(crate) use owner::{FlushedFrames, NavigationOwner};
pub(crate) use rows::{NavigationSource, SourceGroup};

#[cfg(test)]
mod tests;
