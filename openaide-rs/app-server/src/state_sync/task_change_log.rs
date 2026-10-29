//! Recently published `taskChanged` events, kept so a client returning to a Task can
//! receive only what it missed instead of a complete baseline (ADR-0073).
//!
//! The log is a process-local cache owned by the state stream, which records every
//! Task change at its single publication point. It never decides correctness on its
//! own: a miss only means the subscriber gets a baseline.

use std::collections::{HashMap, VecDeque};

use openaide_app_server_protocol::events::TaskChanges;
use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::state::TaskRevisionChanges;

// TODO: tune both bounds from measured `taskChanged` sizes on real streaming Tasks;
// they are conservative first values, not derived from data.
const MAX_TASK_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct TaskChangeLog {
    tasks: HashMap<TaskId, TaskEntries>,
    total_bytes: usize,
    max_task_bytes: usize,
    max_total_bytes: usize,
    sequence: u64,
}

#[derive(Debug, Clone, Default)]
struct TaskEntries {
    /// Highest revision this process published for the Task.
    last_published: u64,
    /// Contiguous revisions ending at `last_published`, oldest first.
    entries: VecDeque<Entry>,
    bytes: usize,
    last_recorded: u64,
}

#[derive(Debug, Clone)]
struct Entry {
    revision: u64,
    changes: TaskChanges,
    bytes: usize,
}

/// What the log knows about the changes after a client's revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChangesSince {
    /// Every published change after the revision, in order, and their encoded size.
    Changes {
        changes: Vec<TaskRevisionChanges>,
        bytes: usize,
    },
    /// This process published nothing for the Task since the log last held it.
    Unknown,
    /// Some published changes after the revision are no longer retained.
    Evicted,
}

impl Default for TaskChangeLog {
    fn default() -> Self {
        Self::with_bounds(MAX_TASK_BYTES, MAX_TOTAL_BYTES)
    }
}

impl TaskChangeLog {
    pub(crate) fn with_bounds(max_task_bytes: usize, max_total_bytes: usize) -> Self {
        Self {
            tasks: HashMap::new(),
            total_bytes: 0,
            max_task_bytes,
            max_total_bytes,
            sequence: 0,
        }
    }

    pub(crate) fn record(&mut self, task_id: &TaskId, revision: u64, changes: &TaskChanges) {
        if changes.removed {
            self.forget(task_id);
            return;
        }
        // Sized as the client will receive it; an unserializable change is never replayed.
        let bytes = serde_json::to_vec(changes).map_or(usize::MAX, |encoded| encoded.len());
        self.sequence += 1;
        let sequence = self.sequence;
        let max_task_bytes = self.max_task_bytes;
        let task = self.tasks.entry(task_id.clone()).or_default();
        let mut released = 0;
        let mut added = 0;
        if !task.entries.is_empty() && revision != task.last_published + 1 {
            // A gap breaks the contiguous run a replay depends on.
            released += task.clear();
        }
        task.last_published = task.last_published.max(revision);
        task.last_recorded = sequence;
        if bytes > max_task_bytes {
            released += task.clear();
        } else {
            task.entries.push_back(Entry {
                revision,
                changes: changes.clone(),
                bytes,
            });
            task.bytes += bytes;
            added = bytes;
            while task.bytes > max_task_bytes {
                released += task.pop_oldest();
            }
        }
        self.total_bytes = self.total_bytes + added - released;
        self.evict_to_total_bound(task_id);
    }

    pub(crate) fn changes_since(&self, task_id: &TaskId, known_revision: u64) -> ChangesSince {
        let Some(task) = self.tasks.get(task_id) else {
            return ChangesSince::Unknown;
        };
        // Revisions up to `known_revision` that are still unpublished reach the new
        // subscriber live, and its ordered ingestion ignores them.
        if known_revision >= task.last_published {
            return ChangesSince::Changes {
                changes: Vec::new(),
                bytes: 0,
            };
        }
        match task.entries.front() {
            Some(first) if first.revision <= known_revision + 1 => {
                let missed = task
                    .entries
                    .iter()
                    .filter(|entry| entry.revision > known_revision);
                ChangesSince::Changes {
                    bytes: missed.clone().map(|entry| entry.bytes).sum(),
                    changes: missed
                        .map(|entry| TaskRevisionChanges {
                            revision: entry.revision,
                            changes: entry.changes.clone(),
                        })
                        .collect(),
                }
            }
            _ => ChangesSince::Evicted,
        }
    }

    pub(crate) fn forget(&mut self, task_id: &TaskId) {
        if let Some(task) = self.tasks.remove(task_id) {
            self.total_bytes -= task.bytes;
        }
    }

    #[cfg(test)]
    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    fn evict_to_total_bound(&mut self, keep: &TaskId) {
        while self.total_bytes > self.max_total_bytes {
            let oldest = self
                .tasks
                .iter()
                .filter(|(task_id, task)| *task_id != keep && !task.entries.is_empty())
                .min_by_key(|(_, task)| task.last_recorded)
                .map(|(task_id, _)| task_id.clone());
            match oldest {
                // Keep the Task's publication high-water mark: it still answers
                // "nothing missed" for clients that are already current.
                Some(task_id) => {
                    let task = self.tasks.get_mut(&task_id).expect("evicted Task exists");
                    self.total_bytes -= task.clear();
                }
                None => {
                    let task = self.tasks.get_mut(keep).expect("recorded Task exists");
                    self.total_bytes -= task.pop_oldest();
                }
            }
        }
    }
}

impl TaskEntries {
    fn clear(&mut self) -> usize {
        let released = self.bytes;
        self.entries.clear();
        self.bytes = 0;
        released
    }

    fn pop_oldest(&mut self) -> usize {
        let released = self.entries.pop_front().map_or(0, |entry| entry.bytes);
        self.bytes -= released;
        released
    }
}

#[cfg(test)]
#[path = "task_change_log_tests.rs"]
mod tests;
