//! Artifact packs: the initial records of many Tool artifacts in one file.
//!
//! A creating commit (a Native Session adoption replays its whole history into one) used to
//! write one fsync'd `<artifact>.journal` per Tool call. It now writes every initial record
//! (sequence 1) of a Task's new artifacts into one `pack-<n>.pack`, with one write, one fsync
//! and one directory sync, before the Task frame that references them.
//!
//! Layout: header (`PACK_MAGIC`, version) | one standard journal frame per artifact
//! (length, JSON `ArtifactFrame`, CRC-32) | one standard frame holding the index (artifact id
//! to record offset) | trailer (index offset, `PACK_END_MAGIC`). A pack without a valid
//! trailer was never completed, so it is ignored: a committed Task frame only references a
//! pack that was fully written and synced first.
//!
//! A pack is immutable. The artifact's own `<id>.journal` file supersedes its pack record once
//! the artifact receives a later frame: the first later frame creates that file atomically
//! holding the pack record plus the new frame, and everything afterwards (append, replay,
//! reconciliation, compaction) is the existing per-artifact path. Packs are removed with the
//! Task directory.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::storage::task_journal::frame::{
    self, crc32, create_directory_durably, io_error, json_error, read_payload, sync_directory,
    DeferredCreates, FaultInjector, FaultPoint, FramedRecord, JournalKind, FORMAT_VERSION,
    MAX_FRAME_BYTES,
};
use crate::storage::task_journal::model::ArtifactFrame;

const PACK_MAGIC: &[u8; 8] = b"OAIDEPK\0";
const PACK_END_MAGIC: &[u8; 8] = b"OAIDEPKE";
const PACK_VERSION: u16 = 1;
const HEADER_LEN: u64 = (PACK_MAGIC.len() + size_of::<u16>()) as u64;
const TRAILER_LEN: u64 = (size_of::<u64>() + PACK_END_MAGIC.len()) as u64;
pub(super) const PACK_EXTENSION: &str = "pack";
const PACK_PREFIX: &str = "pack-";

/// A commit creating at least this many artifacts in one Task packs them; fewer keep the
/// per-artifact file, whose cost is one fsync either way.
const MIN_PACKED_ARTIFACTS: usize = 2;

#[derive(Serialize, Deserialize)]
struct PackIndex {
    format_version: u16,
    entries: Vec<PackIndexEntry>,
}

#[derive(Serialize, Deserialize)]
struct PackIndexEntry {
    artifact_id: String,
    offset: u64,
}

/// What a creating commit wrote, for the commit's diagnostics.
#[derive(Default)]
pub(in crate::storage::task_journal) struct CreateStats {
    pub file_count: usize,
    pub pack_count: usize,
    pub pack_bytes: u64,
}

/// Artifacts a commit creates, made durable together by [`Self::sync`]. Callers must run
/// `sync` before anything that references the artifacts becomes durable.
#[derive(Default)]
pub(in crate::storage::task_journal) struct ArtifactCreates {
    staged: BTreeMap<PathBuf, Vec<StagedArtifact>>,
}

struct StagedArtifact {
    artifact_id: String,
    path: PathBuf,
    frame: ArtifactFrame,
}

impl ArtifactCreates {
    pub(in crate::storage::task_journal) fn stage(
        &mut self,
        path: PathBuf,
        artifact_id: &str,
        frame: ArtifactFrame,
    ) -> Result<(), RuntimeError> {
        let directory = path
            .parent()
            .ok_or_else(|| RuntimeError::Storage("Tool artifact path has no parent".to_string()))?
            .to_path_buf();
        self.staged
            .entry(directory)
            .or_default()
            .push(StagedArtifact {
                artifact_id: artifact_id.to_string(),
                path,
                frame,
            });
        Ok(())
    }

    /// Writes and syncs everything staged: one pack per directory with several artifacts, a
    /// per-artifact file otherwise (those share one batched sync).
    pub(in crate::storage::task_journal) fn sync(
        self,
        kind: JournalKind,
        faults: &FaultInjector,
    ) -> Result<CreateStats, RuntimeError> {
        let mut stats = CreateStats::default();
        let mut files = DeferredCreates::default();
        for (directory, artifacts) in self.staged {
            if artifacts.len() >= MIN_PACKED_ARTIFACTS {
                stats.pack_bytes += write_pack(&directory, &artifacts, kind, faults)?;
                stats.pack_count += 1;
            } else {
                for artifact in &artifacts {
                    files.create(&artifact.path, &artifact.frame, kind, faults)?;
                }
            }
        }
        stats.file_count = files.file_count();
        files.sync(kind, faults)?;
        Ok(stats)
    }
}

fn pack_path(directory: &Path, number: u64) -> PathBuf {
    directory.join(format!("{PACK_PREFIX}{number}.{PACK_EXTENSION}"))
}

fn pack_number(path: &Path) -> Option<u64> {
    if path.extension()?.to_str()? != PACK_EXTENSION {
        return None;
    }
    path.file_stem()?
        .to_str()?
        .strip_prefix(PACK_PREFIX)?
        .parse()
        .ok()
}

/// Pack numbers present in `directory`, ascending. A missing directory has none.
fn pack_numbers(directory: &Path) -> Result<Vec<u64>, std::io::Error> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut numbers = Vec::new();
    for entry in entries {
        if let Some(number) = pack_number(&entry?.path()) {
            numbers.push(number);
        }
    }
    numbers.sort_unstable();
    Ok(numbers)
}

fn encode_frame(payload: &[u8], buffer: &mut Vec<u8>) -> Result<(), RuntimeError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(RuntimeError::Storage(format!(
            "Tool artifact pack record exceeds {MAX_FRAME_BYTES} bytes"
        )));
    }
    buffer.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    buffer.extend_from_slice(payload);
    buffer.extend_from_slice(&crc32(payload).to_le_bytes());
    Ok(())
}

/// Serializes the whole pack in memory, then writes it with one `write` and syncs the file
/// and its directory once. Returns the pack's size in bytes.
fn write_pack(
    directory: &Path,
    artifacts: &[StagedArtifact],
    kind: JournalKind,
    faults: &FaultInjector,
) -> Result<u64, RuntimeError> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(PACK_MAGIC);
    buffer.extend_from_slice(&PACK_VERSION.to_le_bytes());
    let mut entries = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        entries.push(PackIndexEntry {
            artifact_id: artifact.artifact_id.clone(),
            offset: buffer.len() as u64,
        });
        let payload = serde_json::to_vec(&artifact.frame)
            .map_err(|error| json_error("pack_record_encode", error))?;
        encode_frame(&payload, &mut buffer)?;
    }
    let index_offset = buffer.len() as u64;
    let index = serde_json::to_vec(&PackIndex {
        format_version: PACK_VERSION,
        entries,
    })
    .map_err(|error| json_error("pack_index_encode", error))?;
    encode_frame(&index, &mut buffer)?;
    buffer.extend_from_slice(&index_offset.to_le_bytes());
    buffer.extend_from_slice(PACK_END_MAGIC);

    create_directory_durably(directory, kind, faults)?;
    let number = pack_numbers(directory)
        .map_err(|error| io_error("pack_list", error))?
        .last()
        .map_or(1, |last| last + 1);
    faults.check(kind, FaultPoint::CreateOpen)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(pack_path(directory, number))
        .map_err(|error| io_error("create_open", error))?;
    faults.check(kind, FaultPoint::FramePayloadWrite)?;
    file.write_all(&buffer)
        .map_err(|error| io_error("pack_write", error))?;
    faults.check(kind, FaultPoint::FileSync)?;
    faults.record_sync();
    file.sync_all()
        .map_err(|error| io_error("create_sync", error))?;
    sync_directory(directory, kind, FaultPoint::ParentSync, faults)?;
    Ok(buffer.len() as u64)
}

/// Why a pack could not be read, as a metadata-only class.
#[derive(Debug)]
enum PackReadFailure {
    /// No valid trailer: written but never completed (a crash before its commit).
    Incomplete,
    /// Present and complete-looking but failing a checksum, decode, or bounds check.
    Corrupt,
    Io(&'static str),
}

impl PackReadFailure {
    fn class(&self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::Corrupt => "corrupt",
            Self::Io(kind) => kind,
        }
    }

    fn from_io(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::UnexpectedEof => Self::Incomplete,
            std::io::ErrorKind::NotFound => Self::Io("not_found"),
            std::io::ErrorKind::PermissionDenied => Self::Io("permission_denied"),
            _ => Self::Io("other"),
        }
    }
}

impl From<RuntimeError> for PackReadFailure {
    fn from(_: RuntimeError) -> Self {
        Self::Corrupt
    }
}

fn read_index(file: &mut File, file_len: u64) -> Result<PackIndex, PackReadFailure> {
    if file_len < HEADER_LEN + TRAILER_LEN {
        return Err(PackReadFailure::Incomplete);
    }
    let mut header = [0_u8; HEADER_LEN as usize];
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.read_exact(&mut header))
        .map_err(|error| PackReadFailure::from_io(&error))?;
    let mut trailer = [0_u8; TRAILER_LEN as usize];
    file.seek(SeekFrom::End(-(TRAILER_LEN as i64)))
        .and_then(|_| file.read_exact(&mut trailer))
        .map_err(|error| PackReadFailure::from_io(&error))?;
    if &trailer[size_of::<u64>()..] != PACK_END_MAGIC {
        return Err(PackReadFailure::Incomplete);
    }
    if &header[..PACK_MAGIC.len()] != PACK_MAGIC
        || header[PACK_MAGIC.len()..] != PACK_VERSION.to_le_bytes()
    {
        return Err(PackReadFailure::Corrupt);
    }
    let index_offset = u64::from_le_bytes(trailer[..size_of::<u64>()].try_into().expect("8 bytes"));
    let body_end = file_len - TRAILER_LEN;
    if index_offset < HEADER_LEN || index_offset >= body_end {
        return Err(PackReadFailure::Corrupt);
    }
    file.seek(SeekFrom::Start(index_offset))
        .map_err(|error| PackReadFailure::from_io(&error))?;
    let payload =
        read_payload(file, body_end - index_offset, 0)?.ok_or(PackReadFailure::Corrupt)?;
    let index: PackIndex =
        serde_json::from_slice(&payload).map_err(|_| PackReadFailure::Corrupt)?;
    if index.format_version != PACK_VERSION
        || index
            .entries
            .iter()
            .any(|entry| entry.offset < HEADER_LEN || entry.offset >= index_offset)
    {
        return Err(PackReadFailure::Corrupt);
    }
    Ok(index)
}

fn read_record(
    file: &mut File,
    file_len: u64,
    offset: u64,
) -> Result<ArtifactFrame, PackReadFailure> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| PackReadFailure::from_io(&error))?;
    let payload =
        read_payload(file, file_len - TRAILER_LEN - offset, 1)?.ok_or(PackReadFailure::Corrupt)?;
    let frame = ArtifactFrame::decode(&payload)?;
    if frame.format_version != FORMAT_VERSION || frame.sequence != 1 {
        return Err(PackReadFailure::Corrupt);
    }
    Ok(frame)
}

fn read_artifact(path: &Path, artifact_id: &str) -> Result<Option<ArtifactFrame>, PackReadFailure> {
    let mut file = File::open(path).map_err(|error| PackReadFailure::from_io(&error))?;
    let file_len = file
        .metadata()
        .map_err(|error| PackReadFailure::from_io(&error))?
        .len();
    let index = read_index(&mut file, file_len)?;
    let Some(entry) = index
        .entries
        .iter()
        .find(|entry| entry.artifact_id == artifact_id)
    else {
        return Ok(None);
    };
    read_record(&mut file, file_len, entry.offset).map(Some)
}

/// The initial (sequence 1) record of `artifact_id` from the newest pack in `directory` that
/// holds it, or `None`. Newest wins because an uncommitted pack left by a crash can hold the
/// same id as the pack that later committed it, and a committed id is never re-created after
/// a later pack. An unreadable pack is logged (metadata only) and skipped: an artifact it
/// held reports as missing, which the caller turns into "Tool details unavailable".
///
/// TODO: the index of every pack is re-read per lookup. Cache the parsed indexes per Task if
/// Tool detail reads of heavily adopted Tasks show up in profiles.
pub(super) fn find(
    directory: &Path,
    task_id: &str,
    artifact_id: &str,
) -> Result<Option<ArtifactFrame>, RuntimeError> {
    let numbers = pack_numbers(directory).map_err(|error| io_error("pack_list", error))?;
    for number in numbers.into_iter().rev() {
        match read_artifact(&pack_path(directory, number), artifact_id) {
            Ok(Some(frame)) => return Ok(Some(frame)),
            Ok(None) => {}
            Err(failure) => crate::logging::warn(
                "tool_artifact_pack_unavailable",
                serde_json::json!({
                    "task_id": task_id,
                    "pack": number,
                    "failure_kind": failure.class(),
                }),
            ),
        }
    }
    Ok(None)
}

/// Creates `path` holding the packed initial record plus the first later frame, atomically,
/// so a crash leaves either the pack alone or a complete file (never a headerless file that
/// would make the artifact unreadable).
pub(super) fn promote_to_file(
    path: &Path,
    initial: ArtifactFrame,
    next: ArtifactFrame,
    faults: &FaultInjector,
) -> Result<(), RuntimeError> {
    frame::replace_frames_with_faults(path, &[initial, next], faults)
}
