use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::protocol::errors::RuntimeError;

use super::RecoveredTask;

pub(super) const STATUS_FILE: &str = "storage.quarantined";
const HEALTHY: u8 = b'H';
const QUARANTINED: u8 = b'Q';

/// Provisions a fixed-size status byte before a Task can receive journal data.
/// A later disk-full failure can quarantine by overwriting allocated storage.
pub(super) fn ensure_status(task_dir: &Path) -> Result<(), RuntimeError> {
    fs::create_dir_all(task_dir)?;
    let path = task_dir.join(STATUS_FILE);
    if path.try_exists()? {
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    file.write_all(&[HEALTHY])?;
    file.sync_all()?;
    fs::File::open(task_dir)?.sync_all()?;
    Ok(())
}

/// Adds status bytes to journals created before fail-closed markers existed.
pub(super) fn ensure_recovered_statuses(
    tasks_root: &Path,
    recovered: &HashMap<String, RecoveredTask>,
) -> Result<(), RuntimeError> {
    for task_id in recovered.keys() {
        ensure_status(&tasks_root.join(task_id))?;
    }
    Ok(())
}

pub(in crate::storage::task_journal) fn is_quarantined(
    task_dir: &Path,
) -> Result<bool, RuntimeError> {
    let path = task_dir.join(STATUS_FILE);
    if !path.try_exists()? {
        return Ok(false);
    }
    let mut byte = [0_u8; 1];
    let count = fs::File::open(path)?.read(&mut byte)?;
    // The earlier marker format stored a longer `durability_failure` string.
    // Only the exact healthy byte is loadable; unknown contents fail closed.
    Ok(count != 1 || byte[0] != HEALTHY)
}

/// Marks uncertain bytes fail-closed without allocating a new directory entry.
pub(super) fn quarantine(task_dir: &Path) -> Result<(), RuntimeError> {
    let path = task_dir.join(STATUS_FILE);
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&[QUARANTINED])?;
    file.sync_all()?;
    Ok(())
}
