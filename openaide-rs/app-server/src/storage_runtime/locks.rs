use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use fs2::FileExt;
use thiserror::Error;

#[derive(Debug)]
pub struct RuntimeLock {
    path: PathBuf,
    _file: File,
}

impl RuntimeLock {
    pub fn acquire(path: impl Into<PathBuf>) -> Result<LockAcquireOutcome, RuntimeLockError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(LockAcquireOutcome::Acquired(Self { path, _file: file })),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(LockAcquireOutcome::Busy { path })
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for RuntimeLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

#[derive(Debug)]
pub enum LockAcquireOutcome {
    Acquired(RuntimeLock),
    Busy { path: PathBuf },
}

#[derive(Debug, Error)]
pub enum RuntimeLockError {
    #[error("runtime lock I/O failed: {0}")]
    Io(#[from] std::io::Error),
}
