use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::storage::records::TaskRecord;

const CATALOG_FILE: &str = "task.catalog.json";
const CATALOG_SCHEMA_VERSION: u16 = 1;

/// Rebuildable Task Navigation projection. The Task journal remains authoritative;
/// the stamp prevents a stale cache from hiding a later durable journal commit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CatalogEntry {
    schema_version: u16,
    pub task: TaskRecord,
    journal: JournalStamp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct JournalStamp {
    byte_length: u64,
    tail_checksum: u32,
}

pub(super) fn load(task_dir: &Path) -> Result<Option<CatalogEntry>, RuntimeError> {
    let path = catalog_path(task_dir);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RuntimeError::from(error)),
    };
    let entry: CatalogEntry = serde_json::from_slice(&bytes)?;
    if entry.schema_version != CATALOG_SCHEMA_VERSION {
        return Ok(None);
    }
    let journal = task_dir.join(super::store::JOURNAL_FILE);
    if entry.journal != journal_stamp(&journal)? {
        return Ok(None);
    }
    Ok(Some(entry))
}

/// Publishes a disposable cache after the journal commit. It intentionally does
/// not add a durability sync: loss leaves a missing/stale stamp that is rebuilt
/// from the authoritative journal on the next open.
pub(super) fn publish(task_dir: &Path, task: &TaskRecord) -> Result<(), RuntimeError> {
    let journal = task_dir.join(super::store::JOURNAL_FILE);
    let entry = CatalogEntry {
        schema_version: CATALOG_SCHEMA_VERSION,
        task: task.clone(),
        journal: journal_stamp(&journal)?,
    };
    let bytes = serde_json::to_vec_pretty(&entry)?;
    let path = catalog_path(task_dir);
    let temporary = temporary_catalog_path(task_dir);
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.flush()?;
        replace_cache(&temporary, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn journal_stamp(path: &Path) -> Result<JournalStamp, RuntimeError> {
    let mut file = File::open(path)?;
    let byte_length = file.metadata()?.len();
    if byte_length < size_of::<u32>() as u64 {
        return Err(RuntimeError::Storage(
            "Task journal is too short for a catalog stamp".to_string(),
        ));
    }
    file.seek(SeekFrom::End(-(size_of::<u32>() as i64)))?;
    let mut checksum = [0_u8; size_of::<u32>()];
    file.read_exact(&mut checksum)?;
    Ok(JournalStamp {
        byte_length,
        tail_checksum: u32::from_le_bytes(checksum),
    })
}

fn catalog_path(task_dir: &Path) -> PathBuf {
    task_dir.join(CATALOG_FILE)
}

fn temporary_catalog_path(task_dir: &Path) -> PathBuf {
    task_dir.join(format!(".{CATALOG_FILE}.{}", uuid::Uuid::new_v4()))
}

#[cfg(unix)]
fn replace_cache(temporary: &Path, path: &Path) -> Result<(), RuntimeError> {
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(windows)]
fn replace_cache(temporary: &Path, path: &Path) -> Result<(), RuntimeError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING};

    let destination = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both owned UTF-16 buffers are NUL terminated and live through the call.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING,
        )
    } == 0
    {
        return Err(RuntimeError::from(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn replace_cache(_temporary: &Path, _path: &Path) -> Result<(), RuntimeError> {
    Err(RuntimeError::Storage(
        "Task catalog replacement is unsupported on this platform".to_string(),
    ))
}
