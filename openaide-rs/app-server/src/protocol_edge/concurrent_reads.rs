//! Which client requests a transport may run beside its ordered dispatch, and the bound on
//! how many may run at once.
//!
//! A transport dispatches one connection's requests strictly in arrival order, so a slow
//! read (a `git status` behind `fileViewer/changes`, a large image shrink) delays every
//! later request of that connection, including the `state/subscribe` the UI waits on.
//!
//! # Ordering (ADR-0086)
//!
//! Publish order is wire order: every outbound message joins the connection's one outbox
//! when it is produced. Running a request concurrently changes only *when* its reply is
//! produced, never the order of the outbox. What a concurrent request may and may not do:
//!
//! - It is read-only product state: it commits nothing and produces no state event, so no
//!   notification can depend on its position relative to other requests.
//! - Its reply is queued under the protocol lock when the work finishes. It can therefore
//!   arrive after the replies of requests sent later; JSON-RPC clients match replies by id.
//! - State events and the replies of state-changing requests stay on the ordered path, in
//!   request order, so a mutation's frames still precede its reply.
//! - A read that starts after an earlier state-changing request was dispatched sees its
//!   effects, because the ordered path finished that request before accepting the next frame.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use openaide_app_server_protocol::methods::{
    FILE_VIEWER_CHANGES, FILE_VIEWER_DIFF, FILE_VIEWER_EXISTING_FILES, FILE_VIEWER_LIST_DIRECTORY,
    FILE_VIEWER_SEARCH, TASK_IMAGE, TASK_NAVIGATION_SEARCH,
};
use serde_json::json;

use super::stdio::wire::{WireRequest, WireRequestId};

/// Requests that are read-only and already run outside the protocol lock
/// (`SharedRpcGateway::handle_with_completion_clock`), so running them beside the ordered
/// dispatch cannot widen lock contention.
pub(crate) fn is_concurrent_read(method: &str) -> bool {
    matches!(
        method,
        FILE_VIEWER_LIST_DIRECTORY
            | FILE_VIEWER_SEARCH
            | FILE_VIEWER_EXISTING_FILES
            | FILE_VIEWER_CHANGES
            | FILE_VIEWER_DIFF
            | TASK_IMAGE
            | TASK_NAVIGATION_SEARCH
    )
}

/// The method of a well-formed JSON-RPC request frame (not a notification or a response).
pub(crate) fn request_method(message: &serde_json::Value) -> Option<String> {
    let request = serde_json::from_value::<WireRequest>(message.clone()).ok()?;
    (request.jsonrpc == "2.0" && matches!(request.id, WireRequestId::Request(_)))
        .then_some(request.method)
        .flatten()
}

/// One connection may have this many concurrent reads in flight. They are disk, git, or
/// image-decoding bound, so a handful overlaps the typical open burst (list, changes, search)
/// while a client that floods requests cannot spawn unbounded threads. At the cap the
/// dispatcher waits for a slot, which is the old sequential behavior under overload.
pub(crate) const MAX_CONCURRENT_READS: usize = 8;

/// Counts a connection's in-flight concurrent reads.
#[derive(Clone)]
pub(crate) struct ConcurrentReads {
    inner: Arc<(Mutex<usize>, Condvar)>,
    limit: usize,
}

impl Default for ConcurrentReads {
    fn default() -> Self {
        Self::with_limit(MAX_CONCURRENT_READS)
    }
}

impl ConcurrentReads {
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            inner: Arc::default(),
            limit,
        }
    }

    /// Blocks until a slot is free. A wait is logged because it means a read now delays the
    /// ordered dispatch again.
    pub(crate) fn acquire(&self, method: &str) -> ReadSlot {
        let (count, freed) = &*self.inner;
        let mut running = count.lock().expect("concurrent read count poisoned");
        let mut waited_since = None;
        while *running >= self.limit {
            waited_since.get_or_insert_with(Instant::now);
            running = freed.wait(running).expect("concurrent read count poisoned");
        }
        *running += 1;
        drop(running);
        if let Some(waited_since) = waited_since {
            crate::logging::info(
                "concurrent_read_slot_waited",
                json!({
                    "method": method,
                    "limit": self.limit,
                    "wait_ms": waited_since.elapsed().as_millis(),
                }),
            );
        }
        ReadSlot(self.inner.clone())
    }

    #[cfg(test)]
    pub(crate) fn in_flight(&self) -> usize {
        *self.inner.0.lock().expect("concurrent read count poisoned")
    }
}

/// Frees its slot when the read ends, however it ends.
pub(crate) struct ReadSlot(Arc<(Mutex<usize>, Condvar)>);

impl Drop for ReadSlot {
    fn drop(&mut self) {
        let (count, freed) = &*self.0;
        *count.lock().expect("concurrent read count poisoned") -= 1;
        freed.notify_one();
    }
}

#[cfg(test)]
#[path = "concurrent_reads_tests.rs"]
mod tests;
