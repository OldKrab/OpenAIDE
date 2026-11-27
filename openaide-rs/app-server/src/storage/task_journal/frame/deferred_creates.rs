use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::protocol::errors::RuntimeError;

use super::{
    create_directory_durably, io_error, sync_directory, write_frame, FaultInjector, FaultPoint,
    FramedRecord, JournalKind, FORMAT_VERSION, MAGIC,
};

/// New files whose contents and directory entries are made durable together in
/// one [`Self::sync`], instead of three fsyncs per file. A commit that creates
/// many files (one Tool artifact per replayed Tool call) pays one fsync per file
/// plus one per directory. Callers must call `sync` before anything that
/// references the files becomes durable.
#[derive(Default)]
pub(in crate::storage::task_journal) struct DeferredCreates {
    files: Vec<File>,
    parents: Vec<PathBuf>,
}

impl DeferredCreates {
    pub(in crate::storage::task_journal) fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Creates `path` with its first frame, leaving it unsynced until [`Self::sync`].
    pub(in crate::storage::task_journal) fn create<T: FramedRecord>(
        &mut self,
        path: &Path,
        frame: &T,
        kind: JournalKind,
        faults: &FaultInjector,
    ) -> Result<(), RuntimeError> {
        let parent = path
            .parent()
            .ok_or_else(|| RuntimeError::Storage("Task journal path has no parent".to_string()))?;
        if !self.parents.iter().any(|known| known == parent) {
            // Anchored once per directory for the whole batch.
            create_directory_durably(parent, kind, faults)?;
            self.parents.push(parent.to_path_buf());
        }
        faults.check(kind, FaultPoint::CreateOpen)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|error| io_error("create_open", error))?;
        faults.check(kind, FaultPoint::CreateHeaderWrite)?;
        file.write_all(MAGIC)
            .map_err(|error| io_error("create_header_write", error))?;
        file.write_all(&FORMAT_VERSION.to_le_bytes())
            .map_err(|error| io_error("create_header_write", error))?;
        write_frame(&mut file, frame, kind, faults)?;
        self.files.push(file);
        Ok(())
    }

    /// Confirms every created file, then every directory that gained entries. Files sync on
    /// a few threads: concurrent fsyncs share filesystem journal commits, where one after
    /// another each pays a full commit.
    pub(in crate::storage::task_journal) fn sync(
        self,
        kind: JournalKind,
        faults: &FaultInjector,
    ) -> Result<(), RuntimeError> {
        if self.files.is_empty() {
            return Ok(());
        }
        for _ in &self.files {
            faults.check(kind, FaultPoint::FileSync)?;
            faults.record_sync();
        }
        let workers = self.files.len().clamp(1, MAX_PARALLEL_FILE_SYNCS);
        let chunk = self.files.len().div_ceil(workers);
        let failure = std::thread::scope(|scope| {
            let handles = self
                .files
                .chunks(chunk)
                .map(|files| {
                    scope.spawn(move || {
                        files
                            .iter()
                            .try_for_each(|file| file.sync_all())
                            .map_err(|error| io_error("create_sync", error))
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(RuntimeError::Storage(
                            "Task journal create_sync worker panicked".to_string(),
                        ))
                    })
                })
                .find_map(Result::err)
        });
        if let Some(error) = failure {
            return Err(error);
        }
        for parent in &self.parents {
            sync_directory(parent, kind, FaultPoint::ParentSync, faults)?;
        }
        Ok(())
    }
}

const MAX_PARALLEL_FILE_SYNCS: usize = 8;
