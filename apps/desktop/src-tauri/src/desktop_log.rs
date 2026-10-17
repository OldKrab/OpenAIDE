//! The Desktop shell's own diagnostics sink.
//!
//! Every shell event is one metadata-only line. It goes to stderr for a
//! terminal-launched process and to a bounded file in the operating system's log
//! directory, because an app opened from Finder or Start has no stderr anyone
//! reads. A Desktop using a Remote Device runs no App Server here, so this file
//! is its only local record of connection lifecycle.
//!
//! Lines written before `install` reach stderr only. Two Desktop processes can
//! overlap during a restart; both append whole lines, and a rotation racing a
//! write can at worst misplace a line into the newer file.
// TODO: include this file in Support Export. A remote-mode export is built by the
// other computer's App Server, so the Desktop would have to add it to the ZIP it
// saves; until then the file is attached by hand.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "openaide-desktop.log";
const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Total files including the active one.
const FILE_COUNT: usize = 3;

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();

/// Prints one shell event to stderr and, once installed, to the log file.
macro_rules! desktop_log {
    ($($argument:tt)*) => {
        $crate::desktop_log::write(&format!($($argument)*))
    };
}

/// Opens the log file under `directory` for the rest of the process.
pub(crate) fn install(directory: &Path) {
    match Sink::open(directory.join(FILE_NAME), MAX_BYTES) {
        Ok(sink) => {
            let _ = SINK.set(Mutex::new(sink));
        }
        Err(error) => eprintln!("desktop_log_open_failed error_kind={:?}", error.kind()),
    }
}

pub(crate) fn write(line: &str) {
    eprintln!("{line}");
    let Some(sink) = SINK.get() else {
        return;
    };
    if let Ok(mut sink) = sink.lock() {
        sink.append(line);
    }
}

struct Sink {
    path: PathBuf,
    file: File,
    bytes: u64,
    max_bytes: u64,
    /// The file stops after its first failed write instead of reporting each line.
    failed: bool,
}

impl Sink {
    fn open(path: PathBuf, max_bytes: u64) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = open_append(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path,
            file,
            bytes,
            max_bytes,
            failed: false,
        })
    }

    fn append(&mut self, line: &str) {
        if self.failed {
            return;
        }
        if let Err(error) = self.try_append(&format!("{} {line}\n", now_ms())) {
            self.failed = true;
            eprintln!("desktop_log_write_failed error_kind={:?}", error.kind());
        }
    }

    fn try_append(&mut self, record: &str) -> std::io::Result<()> {
        let record_bytes = u64::try_from(record.len()).unwrap_or(u64::MAX);
        if self.bytes > 0 && self.bytes.saturating_add(record_bytes) > self.max_bytes {
            self.rotate()?;
        }
        self.file.write_all(record.as_bytes())?;
        self.bytes = self.bytes.saturating_add(record_bytes);
        Ok(())
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        for index in (1..FILE_COUNT).rev() {
            let from = if index == 1 {
                self.path.clone()
            } else {
                numbered(&self.path, index - 1)
            };
            let to = numbered(&self.path, index);
            match fs::rename(&from, &to) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        self.file = open_append(&self.path)?;
        self.bytes = 0;
        Ok(())
    }
}

fn open_append(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn numbered(path: &Path, index: usize) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "desktop_log_tests.rs"]
mod tests;
