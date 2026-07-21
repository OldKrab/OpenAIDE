use std::fs;
use std::io;
use std::path::Path;

/// Removes the superseded file-backed Task store after journal startup.
///
/// Cleanup is deliberately best-effort: unsupported bytes must not make the
/// App Server unavailable. A later start retries whenever the legacy entry is
/// still present, including after a downgrade created it again.
pub(super) fn remove_after_journal_start(state_root: &Path) {
    let legacy_tasks = state_root.join("tasks");
    let metadata = match fs::symlink_metadata(&legacy_tasks) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            warn("inspect", &error);
            return;
        }
    };

    let removed = if metadata.is_dir() {
        fs::remove_dir_all(&legacy_tasks)
    } else {
        // Do not follow an unexpected symlink or special entry at the legacy
        // path. Removing the entry itself preserves the deletion boundary.
        fs::remove_file(&legacy_tasks)
    };
    if let Err(error) = removed {
        warn("remove", &error);
        return;
    }

    if let Err(error) = sync_state_root(state_root) {
        // The entry is already gone. Report an uncertain metadata flush and
        // rely on the next startup to retry if it reappears after a crash.
        warn("parent_sync", &error);
        return;
    }
    crate::logging::info("legacy_task_storage_removed", serde_json::json!({}));
}

fn warn(operation: &'static str, error: &io::Error) {
    crate::logging::warn(
        "legacy_task_storage_cleanup_failed",
        serde_json::json!({
            "operation": operation,
            "error_kind": format!("{:?}", error.kind()),
        }),
    );
}

#[cfg(unix)]
fn sync_state_root(state_root: &Path) -> io::Result<()> {
    fs::File::open(state_root)?.sync_all()
}

#[cfg(windows)]
fn sync_state_root(_state_root: &Path) -> io::Result<()> {
    // Windows has no portable directory fsync. Deletion is retried whenever
    // the legacy path is observed again on a later startup.
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn sync_state_root(_state_root: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory sync is unsupported on this platform",
    ))
}
