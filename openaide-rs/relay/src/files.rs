//! Private files in the state directory.
//!
//! The service user owns the state directory. Commands an operator runs with `sudo`
//! write there too, so every write hands the new file to the directory's owner;
//! otherwise a root-owned `keys.json` would lock the service out of its own keys.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

/// Replaces `path` with `contents` in one step: a reader sees the old file or the new one,
/// never a partial write. The file is readable by its owner only.
pub fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let temporary = dir.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        give_to_directory_owner(&temporary, dir)?;
        fs::rename(&temporary, path)?;
        // The rename is durable once the directory entry is.
        File::open(dir)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// When root writes into a directory another user owns, the file goes to that user.
fn give_to_directory_owner(file: &Path, dir: &Path) -> io::Result<()> {
    if !nix::unistd::geteuid().is_root() {
        return Ok(());
    }
    let owner = fs::metadata(dir)?;
    std::os::unix::fs::chown(file, Some(owner.uid()), Some(owner.gid()))
}

/// Holds an exclusive advisory lock on a directory until dropped. Every writer of a file in
/// that directory takes it first, so the `serve` process and an operator's command never
/// interleave a read-modify-write. Readers do not lock: [`write_private`] already gives them
/// a whole file.
#[derive(Debug)]
pub struct DirLock(File);

impl DirLock {
    pub fn acquire(dir: &Path) -> io::Result<Self> {
        let handle = File::open(dir)?;
        handle.lock()?;
        Ok(Self(handle))
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
