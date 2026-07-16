use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use ignore::WalkBuilder;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

const DEFAULT_INDEX_CAP: usize = 8;
const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_RESULTS: usize = 12;

/// Shared, watched file indexes keyed by canonical Task Workspace folder.
#[derive(Clone)]
pub(crate) struct WorkspaceFileIndex {
    registry: Arc<Mutex<IndexRegistry>>,
    index_cap: usize,
    idle_ttl: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceFileIndexState {
    Ready,
    Refreshing,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceFileSearch {
    pub(crate) state: WorkspaceFileIndexState,
    pub(crate) paths: Vec<String>,
    pub(crate) notice: Option<String>,
}

struct IndexRegistry {
    entries: HashMap<PathBuf, RegistryEntry>,
}

struct RegistryEntry {
    runtime: Arc<IndexRuntime>,
    _watcher: RecommendedWatcher,
    last_used: Instant,
}

struct IndexRuntime {
    root: PathBuf,
    snapshot: RwLock<IndexSnapshot>,
    dirty: Arc<AtomicBool>,
    refreshing: AtomicBool,
}

#[derive(Default)]
struct IndexSnapshot {
    paths: Vec<String>,
    error: Option<String>,
}

impl Default for WorkspaceFileIndex {
    fn default() -> Self {
        Self::new(DEFAULT_INDEX_CAP, DEFAULT_IDLE_TTL)
    }
}

impl WorkspaceFileIndex {
    pub(crate) fn new(index_cap: usize, idle_ttl: Duration) -> Self {
        Self {
            registry: Arc::new(Mutex::new(IndexRegistry {
                entries: HashMap::new(),
            })),
            index_cap: index_cap.max(1),
            idle_ttl,
        }
    }

    /// Returns bounded, ranked relative paths without exposing watcher or cache state.
    pub(crate) fn search(&self, workspace: &Path, query: &str) -> WorkspaceFileSearch {
        let root = match workspace.canonicalize() {
            Ok(root) if root.is_dir() => root,
            _ => return unavailable("Workspace files are unavailable."),
        };
        let runtime = match self.runtime_for(root) {
            Ok(runtime) => runtime,
            Err(error) => {
                crate::logging::warn(
                    "workspace_file_index_start_failed",
                    serde_json::json!({ "error": error }),
                );
                return unavailable("Workspace file indexing failed.");
            }
        };

        start_refresh_if_needed(runtime.clone());
        let snapshot = runtime
            .snapshot
            .read()
            .expect("workspace file snapshot lock poisoned");
        if snapshot.error.is_some() && !runtime.refreshing.load(Ordering::Acquire) {
            return unavailable("Workspace file indexing failed.");
        }
        let refreshing = runtime.refreshing.load(Ordering::Acquire);
        WorkspaceFileSearch {
            state: if refreshing {
                WorkspaceFileIndexState::Refreshing
            } else {
                WorkspaceFileIndexState::Ready
            },
            paths: rank_paths(&snapshot.paths, query, MAX_RESULTS),
            notice: refreshing.then(|| "Refreshing files…".to_string()),
        }
    }

    /// Drops the cached watcher for a workspace removed by worktree management.
    #[allow(dead_code)] // TODO(#17): call from the worktree-removal workflow when it lands.
    pub(crate) fn forget(&self, canonical_workspace: &Path) {
        self.registry
            .lock()
            .expect("workspace file registry lock poisoned")
            .entries
            .remove(canonical_workspace);
    }

    fn runtime_for(&self, root: PathBuf) -> Result<Arc<IndexRuntime>, String> {
        let now = Instant::now();
        {
            let mut registry = self
                .registry
                .lock()
                .expect("workspace file registry lock poisoned");
            registry
                .entries
                .retain(|_, entry| now.duration_since(entry.last_used) <= self.idle_ttl);
            if let Some(entry) = registry.entries.get_mut(&root) {
                entry.last_used = now;
                return Ok(entry.runtime.clone());
            }
        }

        let entry = build_entry(root.clone())?;
        let runtime = entry.runtime.clone();
        let mut registry = self
            .registry
            .lock()
            .expect("workspace file registry lock poisoned");
        if let Some(existing) = registry.entries.get_mut(&root) {
            existing.last_used = now;
            return Ok(existing.runtime.clone());
        }
        while registry.entries.len() >= self.index_cap {
            let Some(oldest) = registry
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(path, _)| path.clone())
            else {
                break;
            };
            registry.entries.remove(&oldest);
        }
        registry.entries.insert(root, entry);
        Ok(runtime)
    }
}

fn build_entry(root: PathBuf) -> Result<RegistryEntry, String> {
    let dirty = Arc::new(AtomicBool::new(false));
    let callback_dirty = dirty.clone();
    let watcher_armed = Arc::new(AtomicBool::new(false));
    let callback_armed = watcher_armed.clone();
    let callback_root = root.clone();
    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(event) => {
                if callback_armed.load(Ordering::Acquire)
                    && event
                        .paths
                        .iter()
                        .any(|path| !is_git_metadata_path(&callback_root, path))
                {
                    callback_dirty.store(true, Ordering::Release);
                }
            }
            Err(error) => {
                crate::logging::warn(
                    "workspace_file_watch_event_failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
            }
        })
        .map_err(|error| error.to_string())?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;

    let paths = consistent_scan(&root, &dirty)?;
    // Watch registration and the initial walk can produce filesystem events of
    // their own. The completed snapshot already includes them, so only arm
    // invalidation after that snapshot is installed.
    dirty.store(false, Ordering::Release);
    watcher_armed.store(true, Ordering::Release);
    Ok(RegistryEntry {
        runtime: Arc::new(IndexRuntime {
            root,
            snapshot: RwLock::new(IndexSnapshot { paths, error: None }),
            dirty,
            refreshing: AtomicBool::new(false),
        }),
        _watcher: watcher,
        last_used: Instant::now(),
    })
}

fn is_git_metadata_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .is_ok_and(|relative| relative.components().any(|part| part.as_os_str() == ".git"))
}

fn consistent_scan(root: &Path, dirty: &AtomicBool) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for _ in 0..3 {
        dirty.store(false, Ordering::Release);
        paths = discover_paths(root)?;
        if !dirty.load(Ordering::Acquire) {
            return Ok(paths);
        }
    }
    Ok(paths)
}

fn start_refresh_if_needed(runtime: Arc<IndexRuntime>) {
    if !runtime.dirty.load(Ordering::Acquire)
        || runtime
            .refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return;
    }
    std::thread::spawn(move || {
        let result = consistent_scan(&runtime.root, &runtime.dirty);
        let mut snapshot = runtime
            .snapshot
            .write()
            .expect("workspace file snapshot lock poisoned");
        match result {
            Ok(paths) => {
                snapshot.paths = paths;
                snapshot.error = None;
            }
            Err(error) => {
                crate::logging::warn(
                    "workspace_file_index_refresh_failed",
                    serde_json::json!({ "error": error }),
                );
                snapshot.error = Some("refresh failed".to_string());
            }
        }
        runtime.refreshing.store(false, Ordering::Release);
    });
}

fn discover_paths(root: &Path) -> Result<Vec<String>, String> {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(true)
        .ignore(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(true)
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git");

    let mut paths = Vec::new();
    let mut skipped_errors = 0_u64;
    for discovered in builder.build() {
        let entry = match discovered {
            Ok(entry) => entry,
            Err(_) => {
                skipped_errors += 1;
                continue;
            }
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        if let Some(path) = protocol_relative_path(relative) {
            paths.push(path);
        }
    }
    if skipped_errors > 0 {
        crate::logging::warn(
            "workspace_file_index_entries_skipped",
            serde_json::json!({ "error_count": skipped_errors }),
        );
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn protocol_relative_path(path: &Path) -> Option<String> {
    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => segments.push(segment.to_str()?.to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

fn rank_paths(paths: &[String], query: &str, limit: usize) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() {
        let mut shallow = paths.to_vec();
        shallow.sort_by(|left, right| {
            path_depth(left)
                .cmp(&path_depth(right))
                .then_with(|| left.cmp(right))
        });
        shallow.truncate(limit);
        return shallow;
    }

    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let query_lower = query.to_lowercase();
    let mut matches = pattern
        .match_list(paths.iter(), &mut matcher)
        .into_iter()
        .map(|(path, score)| {
            let path_lower = path.to_lowercase();
            let basename = path.rsplit('/').next().unwrap_or(path);
            let basename_lower = basename.to_lowercase();
            let boost = if path == query {
                4_000
            } else if basename == query {
                3_600
            } else if path_lower == query_lower {
                3_400
            } else if basename_lower == query_lower {
                3_200
            } else if path_lower.starts_with(&query_lower) {
                2_800
            } else if basename_lower.starts_with(&query_lower) {
                2_600
            } else {
                0
            };
            (path, score.saturating_add(boost))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|(left_path, left_score), (right_path, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_path.cmp(right_path))
    });
    matches
        .into_iter()
        .take(limit)
        .map(|(path, _)| path.clone())
        .collect()
}

fn path_depth(path: &str) -> usize {
    path.bytes().filter(|byte| *byte == b'/').count()
}

fn unavailable(notice: &str) -> WorkspaceFileSearch {
    WorkspaceFileSearch {
        state: WorkspaceFileIndexState::Unavailable,
        paths: Vec::new(),
        notice: Some(notice.to_string()),
    }
}

#[cfg(test)]
#[path = "workspace_file_index_tests.rs"]
mod tests;
