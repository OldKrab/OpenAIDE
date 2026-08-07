use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub(super) const DEFAULT_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const DEFAULT_MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
pub(super) const DEFAULT_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Clone, Copy, Debug)]
pub(super) struct TracePolicy {
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_age: Duration,
}

impl Default for TracePolicy {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_age: DEFAULT_MAX_AGE,
        }
    }
}

#[derive(Default)]
pub(super) struct PruneOutcome {
    pub retained_bytes: u64,
    pub removed_bytes: u64,
    pub removed_files: usize,
}

struct TraceEntry {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

/// Prunes only closed JSONL traces directly owned by the configured trace root.
pub(super) fn prune(
    root: &Path,
    protected: &HashSet<PathBuf>,
    policy: TracePolicy,
    now: SystemTime,
) -> io::Result<PruneOutcome> {
    let mut entries = match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(trace_entry)
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    entries.sort_by(|left, right| {
        left.modified
            .cmp(&right.modified)
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut outcome = PruneOutcome {
        retained_bytes: entries.iter().map(|entry| entry.bytes).sum(),
        ..PruneOutcome::default()
    };
    let mut removed = HashSet::new();

    for entry in &entries {
        let expired = now
            .duration_since(entry.modified)
            .is_ok_and(|age| age > policy.max_age);
        if expired && !protected.contains(&entry.path) {
            remove_entry(entry, &mut removed, &mut outcome)?;
        }
    }
    for entry in &entries {
        if outcome.retained_bytes <= policy.max_total_bytes {
            break;
        }
        if !removed.contains(&entry.path) && !protected.contains(&entry.path) {
            remove_entry(entry, &mut removed, &mut outcome)?;
        }
    }
    Ok(outcome)
}

fn trace_entry(entry: fs::DirEntry) -> Option<TraceEntry> {
    let path = entry.path();
    if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
        return None;
    }
    let metadata = entry.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(TraceEntry {
        path,
        bytes: metadata.len(),
        modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
    })
}

fn remove_entry(
    entry: &TraceEntry,
    removed: &mut HashSet<PathBuf>,
    outcome: &mut PruneOutcome,
) -> io::Result<()> {
    match fs::remove_file(&entry.path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    removed.insert(entry.path.clone());
    outcome.retained_bytes = outcome.retained_bytes.saturating_sub(entry.bytes);
    outcome.removed_bytes = outcome.removed_bytes.saturating_add(entry.bytes);
    outcome.removed_files = outcome.removed_files.saturating_add(1);
    Ok(())
}
