use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::protocol::errors::RuntimeError;

const MAGIC: &[u8; 8] = b"OAIDETJ\0";
const FORMAT_VERSION: u16 = 1;
const FILE_HEADER_LEN: usize = MAGIC.len() + size_of::<u16>();
const FRAME_LENGTH_LEN: usize = size_of::<u64>();
const FRAME_CHECKSUM_LEN: usize = size_of::<u32>();
const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;

/// Identifies the durable file whose boundary is being exercised. Keeping the
/// scope explicit lets restart tests distinguish an artifact prepare from the
/// Task-journal reference that makes that artifact visible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JournalKind {
    Root,
    Task,
    Artifact,
    ArtifactReference,
    Compaction,
}

/// Every physical boundary at which a storage error can change restart state.
/// Tests arm one exact `(JournalKind, FaultPoint)` pair; production uses the
/// permanently disabled injector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FaultPoint {
    DirectoryParentSync,
    CreateOpen,
    CreateHeaderWrite,
    FrameLengthWrite,
    FramePayloadWrite,
    FrameChecksumWrite,
    FileSync,
    ParentSync,
    AppendOpen,
    TruncateOpen,
    TruncateSetLen,
    TruncateSync,
    CompactionValidate,
    CompactionPublish,
    CompactionPublishParentSync,
    WorkerDispatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArmedFault {
    kind: JournalKind,
    point: FaultPoint,
}

/// Deterministic, one-shot durability fault seam shared with the storage
/// worker. It fails immediately before the selected boundary and records that
/// the intended boundary was reached, avoiding timing- or permission-based
/// fault tests.
#[derive(Default)]
pub(super) struct FaultInjector {
    armed: Mutex<Option<ArmedFault>>,
    sync_calls: AtomicU64,
}

impl FaultInjector {
    pub(super) fn disabled() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(super) fn armed(kind: JournalKind, point: FaultPoint) -> Self {
        Self {
            armed: Mutex::new(Some(ArmedFault { kind, point })),
            sync_calls: AtomicU64::new(0),
        }
    }

    #[cfg(test)]
    pub(super) fn pending(&self) -> bool {
        self.armed
            .lock()
            .expect("Task journal fault injector poisoned")
            .is_some()
    }

    fn check(&self, kind: JournalKind, point: FaultPoint) -> Result<(), RuntimeError> {
        let mut armed = self
            .armed
            .lock()
            .expect("Task journal fault injector poisoned");
        if armed.as_ref() == Some(&ArmedFault { kind, point }) {
            armed.take();
            return Err(RuntimeError::Storage(format!(
                "Task journal injected durability fault at {kind:?}/{point:?}"
            )));
        }
        Ok(())
    }

    pub(super) fn sync_calls(&self) -> u64 {
        self.sync_calls.load(Ordering::Relaxed)
    }

    fn record_sync(&self) {
        self.sync_calls.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn panic_if_armed(&self) {
        if self
            .check(JournalKind::Task, FaultPoint::WorkerDispatch)
            .is_err()
        {
            panic!("injected Task journal worker failure");
        }
    }
}

pub(super) trait FramedRecord: Serialize + DeserializeOwned {
    fn format_version(&self) -> u16;
    fn sequence(&self) -> u64;
}

pub(super) struct ReplayedFrames<T> {
    pub frames: Vec<T>,
    frame_ends: Vec<u64>,
    pub frame_count: usize,
}

/// Returns the exact bytes occupied by a new one-frame journal. Compaction
/// policy stays outside framing without duplicating the physical format.
pub(super) fn one_frame_file_len<T: Serialize>(frame: &T) -> Result<u64, RuntimeError> {
    let payload = serde_json::to_vec(frame).map_err(|error| json_error("encode", error))?;
    let length = FILE_HEADER_LEN
        .checked_add(FRAME_LENGTH_LEN)
        .and_then(|length| length.checked_add(payload.len()))
        .and_then(|length| length.checked_add(FRAME_CHECKSUM_LEN))
        .ok_or_else(|| RuntimeError::Storage("Task journal frame size overflow".to_string()))?;
    u64::try_from(length)
        .map_err(|_| RuntimeError::Storage("Task journal frame size overflow".to_string()))
}

/// Creates the first canonical journal frame and confirms both file contents
/// and the new directory entry before the Task can be published.
#[cfg(test)]
pub(super) fn create<T: FramedRecord>(path: &Path, frame: &T) -> Result<(), RuntimeError> {
    create_with_faults(path, frame, JournalKind::Task, &FaultInjector::disabled())
}

pub(super) fn create_with_faults<T: FramedRecord>(
    path: &Path,
    frame: &T,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Task journal path has no parent".to_string()))?;
    create_directory_durably(parent, kind, faults)?;

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
    faults.check(kind, FaultPoint::FileSync)?;
    faults.record_sync();
    file.sync_all()
        .map_err(|error| io_error("create_sync", error))?;
    sync_directory(parent, kind, FaultPoint::ParentSync, faults)?;
    Ok(())
}

/// Anchors every newly created directory entry before a file inside it can be
/// acknowledged. Syncing only the leaf directory does not make that leaf's
/// own entry durable in its parent after power loss.
pub(super) fn create_directory_durably(
    directory: &Path,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let mut missing = Vec::new();
    let mut cursor = directory;
    while !cursor
        .try_exists()
        .map_err(|error| io_error("create_parent_exists", error))?
    {
        missing.push(cursor.to_path_buf());
        cursor = cursor.parent().ok_or_else(|| {
            RuntimeError::Storage("Task journal directory has no existing ancestor".to_string())
        })?;
    }
    fs::create_dir_all(directory).map_err(|error| io_error("create_parent", error))?;
    if missing.is_empty() {
        let parent = directory.parent().ok_or_else(|| {
            RuntimeError::Storage("Task journal directory has no parent".to_string())
        })?;
        // A previous attempt may have created this directory but failed before
        // anchoring its entry. Reconfirming the parent makes retries safe.
        sync_directory(parent, kind, FaultPoint::DirectoryParentSync, faults)?;
    }
    for created in missing.iter().rev() {
        let parent = created.parent().ok_or_else(|| {
            RuntimeError::Storage("Task journal directory has no parent".to_string())
        })?;
        sync_directory(parent, kind, FaultPoint::DirectoryParentSync, faults)?;
    }
    Ok(())
}

/// Appends and confirms one complete batch before its receipt or publication
/// can resolve.
#[cfg(test)]
pub(super) fn append<T: FramedRecord>(path: &Path, frame: &T) -> Result<(), RuntimeError> {
    append_with_faults(path, frame, JournalKind::Task, &FaultInjector::disabled())
}

pub(super) fn append_with_faults<T: FramedRecord>(
    path: &Path,
    frame: &T,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    faults.check(kind, FaultPoint::AppendOpen)?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| io_error("append_open", error))?;
    write_frame(&mut file, frame, kind, faults)?;
    faults.check(kind, FaultPoint::FileSync)?;
    faults.record_sync();
    file.sync_all()
        .map_err(|error| io_error("append_sync", error))?;
    Ok(())
}

/// Publishes one verified compacted journal with durable platform-aware
/// replacement. Failure never removes or truncates the canonical journal.
pub(super) fn replace_with_faults<T: FramedRecord>(
    path: &Path,
    frame: &T,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Task journal path has no parent".to_string()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("journal");
    let temporary = parent.join(format!(".{file_name}.compact-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        create_with_faults(&temporary, frame, JournalKind::Compaction, faults)?;
        faults.check(JournalKind::Compaction, FaultPoint::CompactionValidate)?;
        let verified: ReplayedFrames<T> = replay(&temporary)?;
        if verified.frames.len() != 1 {
            return Err(RuntimeError::Storage(
                "Task journal compaction validation failed".to_string(),
            ));
        }
        faults.check(JournalKind::Compaction, FaultPoint::CompactionPublish)?;
        durable_replace(&temporary, path)?;
        sync_directory(
            parent,
            JournalKind::Compaction,
            FaultPoint::CompactionPublishParentSync,
            faults,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Replays complete verified frames. Only an incomplete final frame is removed;
/// checksum, sequence, and payload failures remain visible to the caller.
pub(super) fn replay<T: FramedRecord>(path: &Path) -> Result<ReplayedFrames<T>, RuntimeError> {
    replay_internal(path, true)
}

/// Validates and indexes a journal without retaining decoded frame payloads.
/// Startup artifact reconciliation must scale with frame count, not lifetime
/// terminal-output bytes across the complete state root.
pub(super) fn scan<T: FramedRecord>(path: &Path) -> Result<ReplayedFrames<T>, RuntimeError> {
    replay_internal(path, false)
}

fn replay_internal<T: FramedRecord>(
    path: &Path,
    retain_frames: bool,
) -> Result<ReplayedFrames<T>, RuntimeError> {
    let mut file = File::open(path).map_err(|error| io_error("replay_open", error))?;
    let file_len = file
        .metadata()
        .map_err(|error| io_error("replay_metadata", error))?
        .len();
    let mut header = [0_u8; FILE_HEADER_LEN];
    if let Err(error) = file.read_exact(&mut header) {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            return Err(RuntimeError::Storage(
                "Invalid Task journal header".to_string(),
            ));
        }
        return Err(io_error("replay_header", error));
    }
    validate_header(&header)?;

    let mut cursor = FILE_HEADER_LEN as u64;
    let mut complete_len = cursor;
    let mut expected_sequence = 1_u64;
    let mut frames = Vec::new();
    let mut frame_ends = Vec::new();

    while cursor < file_len {
        let remaining = file_len - cursor;
        if remaining < FRAME_LENGTH_LEN as u64 {
            break;
        }
        let mut length_bytes = [0_u8; FRAME_LENGTH_LEN];
        file.read_exact(&mut length_bytes)
            .map_err(|error| io_error("replay_length", error))?;
        let payload_len = u64::from_le_bytes(length_bytes);
        let payload_len = usize::try_from(payload_len).map_err(|_| {
            RuntimeError::Storage("Task journal frame length does not fit memory".to_string())
        })?;
        if payload_len > MAX_FRAME_BYTES {
            return Err(RuntimeError::Storage(format!(
                "Task journal frame exceeds {MAX_FRAME_BYTES} bytes"
            )));
        }
        let frame_len = FRAME_LENGTH_LEN
            .checked_add(payload_len)
            .and_then(|value| value.checked_add(FRAME_CHECKSUM_LEN))
            .ok_or_else(|| {
                RuntimeError::Storage("Task journal frame length overflow".to_string())
            })?;
        if remaining < frame_len as u64 {
            break;
        }

        let mut payload = vec![0_u8; payload_len];
        file.read_exact(&mut payload)
            .map_err(|error| io_error("replay_payload", error))?;
        let mut checksum_bytes = [0_u8; FRAME_CHECKSUM_LEN];
        file.read_exact(&mut checksum_bytes)
            .map_err(|error| io_error("replay_checksum", error))?;
        let expected_checksum = u32::from_le_bytes(checksum_bytes);
        let actual_checksum = crc32(&payload);
        if actual_checksum != expected_checksum {
            return Err(RuntimeError::Storage(format!(
                "Task journal checksum mismatch at sequence {expected_sequence}"
            )));
        }
        let frame: T =
            serde_json::from_slice(&payload).map_err(|error| json_error("frame_decode", error))?;
        if frame.format_version() != FORMAT_VERSION {
            return Err(RuntimeError::Storage(format!(
                "Unsupported journal frame version {}",
                frame.format_version()
            )));
        }
        if frame.sequence() != expected_sequence {
            return Err(RuntimeError::Storage(format!(
                "Task journal sequence gap: expected {expected_sequence}, found {}",
                frame.sequence()
            )));
        }

        if retain_frames {
            frames.push(frame);
        }
        expected_sequence += 1;
        cursor += frame_len as u64;
        complete_len = cursor;
        frame_ends.push(complete_len);
    }

    let discarded_tail_bytes = file_len.saturating_sub(complete_len);
    if discarded_tail_bytes > 0 {
        let file = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|error| io_error("tail_recovery_open", error))?;
        file.set_len(complete_len)
            .map_err(|error| io_error("tail_recovery_truncate", error))?;
        file.sync_all()
            .map_err(|error| io_error("tail_recovery_sync", error))?;
        crate::logging::warn(
            "task_journal_incomplete_tail_discarded",
            serde_json::json!({
                "stage": "replay_tail_recovery",
                "storage_kind": "framed_journal",
                "discarded_bytes": discarded_tail_bytes,
            }),
        );
    }

    let frame_count = frame_ends.len();
    Ok(ReplayedFrames {
        frames,
        frame_ends,
        frame_count,
    })
}

/// Removes complete but uncommitted frames, retaining the shared file header.
/// Artifact recovery uses this after consulting the authoritative Task head.
pub(super) fn truncate_after<T>(
    path: &Path,
    replayed: &ReplayedFrames<T>,
    retained_frames: usize,
) -> Result<(), RuntimeError> {
    truncate_after_with_faults(
        path,
        replayed,
        retained_frames,
        JournalKind::Artifact,
        &FaultInjector::disabled(),
    )
}

pub(super) fn truncate_after_with_faults<T>(
    path: &Path,
    replayed: &ReplayedFrames<T>,
    retained_frames: usize,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    if retained_frames >= replayed.frame_count {
        return Ok(());
    }
    let retained_len = if retained_frames == 0 {
        FILE_HEADER_LEN as u64
    } else {
        replayed.frame_ends[retained_frames - 1]
    };
    faults.check(kind, FaultPoint::TruncateOpen)?;
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| io_error("reconcile_open", error))?;
    faults.check(kind, FaultPoint::TruncateSetLen)?;
    file.set_len(retained_len)
        .map_err(|error| io_error("reconcile_truncate", error))?;
    faults.check(kind, FaultPoint::TruncateSync)?;
    faults.record_sync();
    file.sync_all()
        .map_err(|error| io_error("reconcile_sync", error))?;
    crate::logging::warn(
        "journal_uncommitted_tail_discarded",
        serde_json::json!({
            "stage": "artifact_tail_reconciliation",
            "storage_kind": "framed_journal",
            "discarded_frames": replayed.frame_count - retained_frames,
        }),
    );
    Ok(())
}

fn validate_header(bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.len() < FILE_HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
        return Err(RuntimeError::Storage(
            "Invalid Task journal header".to_string(),
        ));
    }
    let version = u16::from_le_bytes(
        bytes[MAGIC.len()..FILE_HEADER_LEN]
            .try_into()
            .expect("format version slice"),
    );
    if version != FORMAT_VERSION {
        return Err(RuntimeError::Storage(format!(
            "Unsupported Task journal version {version}"
        )));
    }
    Ok(())
}

fn write_frame<T: FramedRecord>(
    file: &mut File,
    frame: &T,
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    let payload = serde_json::to_vec(frame).map_err(|error| json_error("frame_encode", error))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(RuntimeError::Storage(format!(
            "Task journal frame exceeds {MAX_FRAME_BYTES} bytes"
        )));
    }
    faults.check(kind, FaultPoint::FrameLengthWrite)?;
    file.write_all(&(payload.len() as u64).to_le_bytes())
        .map_err(|error| io_error("frame_length_write", error))?;
    faults.check(kind, FaultPoint::FramePayloadWrite)?;
    file.write_all(&payload)
        .map_err(|error| io_error("frame_payload_write", error))?;
    faults.check(kind, FaultPoint::FrameChecksumWrite)?;
    file.write_all(&crc32(&payload).to_le_bytes())
        .map_err(|error| io_error("frame_checksum_write", error))?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(
    path: &Path,
    kind: JournalKind,
    point: FaultPoint,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    faults.check(kind, point)?;
    faults.record_sync();
    File::open(path)
        .map_err(|error| io_error("parent_sync_open", error))?
        .sync_all()
        .map_err(|error| io_error("parent_sync", error))?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(
    _path: &Path,
    kind: JournalKind,
    point: FaultPoint,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    faults.check(kind, point)?;
    // Windows FlushFileBuffers (used by File::sync_all above) flushes the file
    // and its metadata; compaction publication additionally uses
    // MOVEFILE_WRITE_THROUGH. Windows does not expose a portable directory
    // fsync equivalent.
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn sync_directory(
    _path: &Path,
    kind: JournalKind,
    point: FaultPoint,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    faults.check(kind, point)?;
    Err(RuntimeError::Storage(
        "Task journal parent_sync failed (kind=unsupported_platform)".to_string(),
    ))
}

#[cfg(unix)]
fn durable_replace(temporary: &Path, path: &Path) -> Result<(), RuntimeError> {
    fs::rename(temporary, path).map_err(|error| io_error("compact_replace", error))
}

#[cfg(windows)]
fn durable_replace(temporary: &Path, path: &Path) -> Result<(), RuntimeError> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let replaced = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are owned, NUL-terminated UTF-16 buffers that remain
    // alive for the call. MoveFileExW explicitly supports replacement plus
    // write-through, so publication is durable before this boundary returns.
    let result = unsafe {
        MoveFileExW(
            replacement.as_ptr(),
            replaced.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(io_error("compact_replace", io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn durable_replace(_temporary: &Path, _path: &Path) -> Result<(), RuntimeError> {
    Err(RuntimeError::Storage(
        "Task journal compact_replace failed (kind=unsupported_platform)".to_string(),
    ))
}

fn io_error(stage: &'static str, error: io::Error) -> RuntimeError {
    RuntimeError::Storage(format!(
        "Task journal {stage} failed (kind={})",
        io_error_kind(error.kind())
    ))
}

fn io_error_kind(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::AlreadyExists => "already_exists",
        io::ErrorKind::InvalidInput => "invalid_input",
        io::ErrorKind::InvalidData => "invalid_data",
        io::ErrorKind::TimedOut => "timed_out",
        io::ErrorKind::WriteZero => "write_zero",
        io::ErrorKind::UnexpectedEof => "unexpected_eof",
        io::ErrorKind::OutOfMemory => "out_of_memory",
        io::ErrorKind::Unsupported => "unsupported",
        _ => "other",
    }
}

fn json_error(stage: &'static str, error: serde_json::Error) -> RuntimeError {
    let kind = match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    };
    RuntimeError::Storage(format!("Task journal {stage} failed (kind={kind})"))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
