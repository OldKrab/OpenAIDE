//! One keeper per state root. The keeper holds an exclusive lock on its lease file
//! for as long as it runs and writes its process id and the App Server it keeps
//! into it. The operating system releases the lock when the process ends, so a
//! lease whose lock is free is stale whatever the file says.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LeaseHolder {
    pub(crate) pid: u32,
    pub(crate) server_id: String,
}

pub(crate) enum LeaseAttempt {
    Acquired(Lease),
    /// Another keeper runs. Its record is absent while it is still writing it.
    Held(Option<LeaseHolder>),
}

/// The lock, held until dropped.
pub(crate) struct Lease {
    _file: File,
}

pub(crate) fn lease_path(runtime_root: &Path, fingerprint: &str) -> PathBuf {
    runtime_root.join(format!("{fingerprint}.keeper"))
}

pub(crate) fn try_acquire(path: &Path, holder: &LeaseHolder) -> std::io::Result<LeaseAttempt> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if file.try_lock_exclusive().is_err() {
        return Ok(LeaseAttempt::Held(read_holder(&mut file)));
    }
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&serde_json::to_vec(holder).map_err(std::io::Error::other)?)?;
    file.sync_data()?;
    Ok(LeaseAttempt::Acquired(Lease { _file: file }))
}

/// The running keeper, or `None` when no keeper holds the lease.
pub(crate) fn current_holder(path: &Path) -> std::io::Result<Option<LeaseHolder>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if file.try_lock_shared().is_ok() {
        let _ = FileExt::unlock(&file);
        return Ok(None);
    }
    Ok(read_holder(&mut file))
}

fn read_holder(file: &mut File) -> Option<LeaseHolder> {
    let mut text = String::new();
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
#[path = "lease_tests.rs"]
mod tests;
