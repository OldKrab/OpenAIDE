use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::protocol::errors::RuntimeError;

const MAGIC: &[u8; 8] = b"OAIDETJ\0";
const FORMAT_VERSION: u16 = 1;
const FILE_HEADER_LEN: usize = MAGIC.len() + size_of::<u16>();
const FRAME_LENGTH_LEN: usize = size_of::<u64>();
const FRAME_CHECKSUM_LEN: usize = size_of::<u32>();
const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;

pub(super) trait FramedRecord: Serialize + DeserializeOwned {
    fn format_version(&self) -> u16;
    fn sequence(&self) -> u64;
}

pub(super) struct ReplayedFrames<T> {
    pub frames: Vec<T>,
    frame_ends: Vec<u64>,
}

/// Creates the first canonical journal frame and confirms both file contents
/// and the new directory entry before the Task can be published.
pub(super) fn create<T: FramedRecord>(path: &Path, frame: &T) -> Result<(), RuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Storage("Task journal path has no parent".to_string()))?;
    fs::create_dir_all(parent)?;

    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&FORMAT_VERSION.to_le_bytes())?;
    write_frame(&mut file, frame)?;
    file.sync_all()?;
    sync_directory(parent)?;
    Ok(())
}

/// Appends and confirms one complete batch before its receipt or publication
/// can resolve.
pub(super) fn append<T: FramedRecord>(path: &Path, frame: &T) -> Result<(), RuntimeError> {
    let mut file = OpenOptions::new().append(true).open(path)?;
    write_frame(&mut file, frame)?;
    file.sync_all()?;
    Ok(())
}

/// Replays complete verified frames. Only an incomplete final frame is removed;
/// checksum, sequence, and payload failures remain visible to the caller.
pub(super) fn replay<T: FramedRecord>(path: &Path) -> Result<ReplayedFrames<T>, RuntimeError> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    validate_header(path, &bytes)?;

    let mut cursor = FILE_HEADER_LEN;
    let mut complete_len = cursor;
    let mut expected_sequence = 1_u64;
    let mut frames = Vec::new();
    let mut frame_ends = Vec::new();

    while cursor < bytes.len() {
        let remaining = bytes.len() - cursor;
        if remaining < FRAME_LENGTH_LEN {
            break;
        }
        let payload_len = u64::from_le_bytes(
            bytes[cursor..cursor + FRAME_LENGTH_LEN]
                .try_into()
                .expect("frame length slice"),
        );
        let payload_len = usize::try_from(payload_len).map_err(|_| {
            RuntimeError::Storage(format!(
                "Task journal frame length does not fit memory: {}",
                path.display()
            ))
        })?;
        if payload_len > MAX_FRAME_BYTES {
            return Err(RuntimeError::Storage(format!(
                "Task journal frame exceeds {MAX_FRAME_BYTES} bytes: {}",
                path.display()
            )));
        }
        let frame_len = FRAME_LENGTH_LEN
            .checked_add(payload_len)
            .and_then(|value| value.checked_add(FRAME_CHECKSUM_LEN))
            .ok_or_else(|| {
                RuntimeError::Storage("Task journal frame length overflow".to_string())
            })?;
        if remaining < frame_len {
            break;
        }

        let payload_start = cursor + FRAME_LENGTH_LEN;
        let payload_end = payload_start + payload_len;
        let expected_checksum = u32::from_le_bytes(
            bytes[payload_end..payload_end + FRAME_CHECKSUM_LEN]
                .try_into()
                .expect("frame checksum slice"),
        );
        let actual_checksum = crc32(&bytes[payload_start..payload_end]);
        if actual_checksum != expected_checksum {
            return Err(RuntimeError::Storage(format!(
                "Task journal checksum mismatch at sequence {expected_sequence}: {}",
                path.display()
            )));
        }
        let frame: T = serde_json::from_slice(&bytes[payload_start..payload_end])?;
        if frame.format_version() != FORMAT_VERSION {
            return Err(RuntimeError::Storage(format!(
                "Unsupported journal frame version {}: {}",
                frame.format_version(),
                path.display()
            )));
        }
        if frame.sequence() != expected_sequence {
            return Err(RuntimeError::Storage(format!(
                "Task journal sequence gap: expected {expected_sequence}, found {}: {}",
                frame.sequence(),
                path.display()
            )));
        }

        frames.push(frame);
        expected_sequence += 1;
        cursor += frame_len;
        complete_len = cursor;
        frame_ends.push(complete_len as u64);
    }

    let discarded_tail_bytes = bytes.len().saturating_sub(complete_len);
    if discarded_tail_bytes > 0 {
        let file = OpenOptions::new().write(true).open(path)?;
        file.set_len(complete_len as u64)?;
        file.sync_all()?;
        crate::logging::warn(
            "task_journal_incomplete_tail_discarded",
            serde_json::json!({
                "path": path.display().to_string(),
                "discarded_bytes": discarded_tail_bytes,
            }),
        );
    }

    Ok(ReplayedFrames { frames, frame_ends })
}

/// Removes complete but uncommitted frames, retaining the shared file header.
/// Artifact recovery uses this after consulting the authoritative Task head.
pub(super) fn truncate_after<T>(
    path: &Path,
    replayed: &ReplayedFrames<T>,
    retained_frames: usize,
) -> Result<(), RuntimeError> {
    if retained_frames >= replayed.frames.len() {
        return Ok(());
    }
    let retained_len = if retained_frames == 0 {
        FILE_HEADER_LEN as u64
    } else {
        replayed.frame_ends[retained_frames - 1]
    };
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_len(retained_len)?;
    file.sync_all()?;
    crate::logging::warn(
        "journal_uncommitted_tail_discarded",
        serde_json::json!({
            "path": path.display().to_string(),
            "discarded_frames": replayed.frames.len() - retained_frames,
        }),
    );
    Ok(())
}

fn validate_header(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.len() < FILE_HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
        return Err(RuntimeError::Storage(format!(
            "Invalid Task journal header: {}",
            path.display()
        )));
    }
    let version = u16::from_le_bytes(
        bytes[MAGIC.len()..FILE_HEADER_LEN]
            .try_into()
            .expect("format version slice"),
    );
    if version != FORMAT_VERSION {
        return Err(RuntimeError::Storage(format!(
            "Unsupported Task journal version {version}: {}",
            path.display()
        )));
    }
    Ok(())
}

fn write_frame<T: FramedRecord>(file: &mut File, frame: &T) -> Result<(), RuntimeError> {
    let payload = serde_json::to_vec(frame)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(RuntimeError::Storage(format!(
            "Task journal frame exceeds {MAX_FRAME_BYTES} bytes"
        )));
    }
    file.write_all(&(payload.len() as u64).to_le_bytes())?;
    file.write_all(&payload)?;
    file.write_all(&crc32(&payload).to_le_bytes())?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), RuntimeError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), RuntimeError> {
    // File::sync_all confirms file contents on supported non-Unix targets. The
    // compaction slice supplies the platform-specific durable replacement.
    Ok(())
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
