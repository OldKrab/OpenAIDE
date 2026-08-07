use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const DEFAULT_FILE_COUNT: usize = 4;

#[derive(Clone, Copy)]
pub(super) struct RotationPolicy {
    pub max_bytes: u64,
    /// Total files including the active log.
    pub file_count: usize,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            file_count: DEFAULT_FILE_COUNT,
        }
    }
}

/// Owns bounded append-only logging for one process-owned diagnostics file.
pub(super) struct RotatingLogFile {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    policy: RotationPolicy,
}

impl RotatingLogFile {
    pub fn open(path: PathBuf, policy: RotationPolicy) -> io::Result<Self> {
        let parent = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        if file_len(&path)? >= policy.max_bytes && file_len(&path)? > 0 {
            rotate_paths(&path, policy.file_count)?;
        }
        let file = open_append(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path,
            file: Some(file),
            bytes,
            policy,
        })
    }

    pub fn append_line(&mut self, line: &str) -> io::Result<()> {
        let line_bytes = u64::try_from(line.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        if line_bytes > self.policy.max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "log record exceeds the rotation file limit",
            ));
        }
        if self.bytes > 0 && self.bytes.saturating_add(line_bytes) > self.policy.max_bytes {
            self.rotate()?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("rotating log file is closed"))?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        self.bytes = self.bytes.saturating_add(line_bytes);
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        // Windows cannot rename an open file. Drop the handle before moving it.
        self.file.take();
        if let Err(error) = rotate_paths(&self.path, self.policy.file_count) {
            self.file = Some(open_append(&self.path)?);
            self.bytes = file_len(&self.path)?;
            return Err(error);
        }
        self.file = Some(open_append(&self.path)?);
        self.bytes = 0;
        Ok(())
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn file_len(path: &Path) -> io::Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn rotate_paths(path: &Path, file_count: usize) -> io::Result<()> {
    if file_count <= 1 {
        return remove_if_present(path);
    }
    for generation in (1..file_count).rev() {
        let source = if generation == 1 {
            path.to_path_buf()
        } else {
            backup_path(path, generation - 1)
        };
        let destination = backup_path(path, generation);
        remove_if_present(&destination)?;
        match fs::rename(&source, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn backup_path(path: &Path, generation: usize) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_os_string();
    value.push(format!(".{generation}"));
    PathBuf::from(value)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
