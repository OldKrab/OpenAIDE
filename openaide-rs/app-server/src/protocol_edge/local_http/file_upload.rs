use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openaide_app_server_protocol::ids::ClientInstanceId;
use thiserror::Error;

pub(crate) const MAX_UPLOAD_CHUNK_BYTES: usize = 512 * 1024;
const STALE_UPLOAD_AGE: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Default)]
pub(crate) struct ChunkUploadRegistry {
    sessions: Arc<Mutex<HashMap<ChunkUploadKey, ChunkUploadSession>>>,
}

pub(crate) struct ChunkUploadRequest<'a> {
    pub client_instance_id: &'a ClientInstanceId,
    pub upload_id: &'a str,
    pub task_id: &'a str,
    pub file_name: &'a str,
    pub total_size: usize,
    pub offset: usize,
    pub bytes: &'a [u8],
}

pub(crate) enum AppendChunkOutcome {
    Partial {
        received: usize,
    },
    Complete {
        temporary: PendingUpload,
        task_id: String,
        file_name: String,
    },
}

#[derive(Debug, Error)]
pub(crate) enum ChunkUploadError {
    #[error("upload id is invalid")]
    InvalidUploadId,
    #[error("upload chunk exceeds the {max} byte limit")]
    ChunkTooLarge { max: usize },
    #[error("upload session was not found")]
    MissingSession,
    #[error("upload metadata changed during transfer")]
    MetadataMismatch,
    #[error("upload chunk offset is invalid; expected {expected}")]
    OffsetMismatch { expected: usize },
    #[error("upload chunk exceeds the declared file size")]
    SizeExceeded,
    #[error("upload session state is unavailable")]
    StateUnavailable,
    #[error("upload temporary file failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ChunkUploadKey {
    client_instance_id: ClientInstanceId,
    upload_id: String,
}

struct ChunkUploadSession {
    task_id: String,
    file_name: String,
    total_size: usize,
    received: usize,
    temporary: PendingUpload,
    last_activity: Instant,
}

/// Owns an incomplete upload and removes its partial file unless committed.
pub(crate) struct PendingUpload {
    file: Option<File>,
    path: PathBuf,
    committed: bool,
}

impl PendingUpload {
    #[cfg(test)]
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub(crate) fn keep(mut self) -> Result<(File, PathBuf), std::io::Error> {
        self.flush()?;
        self.committed = true;
        Ok((
            self.file
                .take()
                .expect("pending upload must own its file until commit"),
            self.path.clone(),
        ))
    }
}

impl Write for PendingUpload {
    fn write(&mut self, buffer: &[u8]) -> Result<usize, std::io::Error> {
        self.file
            .as_mut()
            .expect("pending upload must own its file until commit")
            .write(buffer)
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.file
            .as_mut()
            .expect("pending upload must own its file until commit")
            .flush()
    }
}

impl Drop for PendingUpload {
    fn drop(&mut self) {
        if !self.committed {
            self.file.take();
            let _ = std::fs::remove_file(&self.path);
            if let Some(directory) = self.path.parent() {
                let _ = std::fs::remove_dir(directory);
            }
        }
    }
}

impl ChunkUploadRegistry {
    /// Appends one authenticated chunk while enforcing exact sequential offsets.
    pub(crate) fn append(
        &self,
        request: ChunkUploadRequest<'_>,
    ) -> Result<AppendChunkOutcome, ChunkUploadError> {
        validate_upload_id(request.upload_id)?;
        if request.bytes.len() > MAX_UPLOAD_CHUNK_BYTES {
            return Err(ChunkUploadError::ChunkTooLarge {
                max: MAX_UPLOAD_CHUNK_BYTES,
            });
        }
        if request.offset > request.total_size
            || request.bytes.len() > request.total_size - request.offset
        {
            return Err(ChunkUploadError::SizeExceeded);
        }

        let key = ChunkUploadKey {
            client_instance_id: request.client_instance_id.clone(),
            upload_id: request.upload_id.to_string(),
        };
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| ChunkUploadError::StateUnavailable)?;
        let now = Instant::now();
        sessions.retain(|_, session| now.duration_since(session.last_activity) < STALE_UPLOAD_AGE);
        if request.offset == 0 && !sessions.contains_key(&key) {
            sessions.insert(
                key.clone(),
                ChunkUploadSession {
                    task_id: request.task_id.to_string(),
                    file_name: request.file_name.to_string(),
                    total_size: request.total_size,
                    received: 0,
                    temporary: temporary_upload(request.task_id, request.file_name)?,
                    last_activity: now,
                },
            );
        }

        let session = sessions
            .get_mut(&key)
            .ok_or(ChunkUploadError::MissingSession)?;
        if session.task_id != request.task_id
            || session.file_name != request.file_name
            || session.total_size != request.total_size
        {
            return Err(ChunkUploadError::MetadataMismatch);
        }
        if session.received != request.offset {
            return Err(ChunkUploadError::OffsetMismatch {
                expected: session.received,
            });
        }
        session.temporary.write_all(request.bytes)?;
        session.received += request.bytes.len();
        session.last_activity = now;
        if session.received < session.total_size {
            return Ok(AppendChunkOutcome::Partial {
                received: session.received,
            });
        }

        let mut session = sessions
            .remove(&key)
            .expect("completed upload session must remain registered");
        session.temporary.flush()?;
        Ok(AppendChunkOutcome::Complete {
            temporary: session.temporary,
            task_id: session.task_id,
            file_name: session.file_name,
        })
    }

    /// Cancels one client's partial upload and drops its temporary file.
    pub(crate) fn cancel(&self, client_instance_id: &ClientInstanceId, upload_id: &str) -> bool {
        if validate_upload_id(upload_id).is_err() {
            return false;
        }
        let Ok(mut sessions) = self.sessions.lock() else {
            return false;
        };
        sessions
            .remove(&ChunkUploadKey {
                client_instance_id: client_instance_id.clone(),
                upload_id: upload_id.to_string(),
            })
            .is_some()
    }
}

pub(crate) fn temporary_upload(
    task_id: &str,
    file_name: &str,
) -> Result<PendingUpload, std::io::Error> {
    let upload_directory = std::env::temp_dir()
        .join("openaide")
        .join("uploads")
        .join(safe_upload_directory_name(task_id));
    create_private_directory(&upload_directory)?;
    let safe_name = safe_upload_file_name(file_name);
    for collision_index in 1_u64.. {
        let path = upload_directory.join(upload_file_name(&safe_name, collision_index));
        match create_private_file(&path) {
            Ok(file) => {
                return Ok(PendingUpload {
                    file: Some(file),
                    path,
                    committed: false,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    unreachable!("the upload collision counter is unbounded")
}

fn create_private_directory(path: &std::path::Path) -> Result<(), std::io::Error> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn create_private_file(path: &std::path::Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn safe_upload_directory_name(task_id: &str) -> String {
    let sanitized = task_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(160)
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown-task".to_string()
    } else {
        sanitized
    }
}

fn upload_file_name(file_name: &str, collision_index: u64) -> String {
    if collision_index == 1 {
        return file_name.to_string();
    }
    let collision_suffix = format!("-{collision_index}");
    let (stem, extension) = file_name
        .rsplit_once('.')
        .filter(|(stem, extension)| !stem.is_empty() && !extension.is_empty())
        .map(|(stem, extension)| (stem, format!(".{extension}")))
        .unwrap_or((file_name, String::new()));
    let stem_characters =
        160_usize.saturating_sub(collision_suffix.chars().count() + extension.chars().count());
    format!(
        "{}{collision_suffix}{extension}",
        stem.chars().take(stem_characters).collect::<String>()
    )
}

fn validate_upload_id(upload_id: &str) -> Result<(), ChunkUploadError> {
    if upload_id.is_empty()
        || upload_id.len() > 128
        || !upload_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ChunkUploadError::InvalidUploadId);
    }
    Ok(())
}

pub(crate) fn safe_upload_file_name(file_name: &str) -> String {
    let basename = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    let sanitized = basename
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_end_matches([' ', '.']);
    if sanitized.is_empty() || matches!(sanitized, "." | "..") {
        "Attached file".to_string()
    } else if is_windows_reserved_file_name(sanitized) {
        shorten_upload_file_name(&format!("_{sanitized}"))
    } else {
        shorten_upload_file_name(sanitized)
    }
}

fn shorten_upload_file_name(file_name: &str) -> String {
    const MAX_FILE_NAME_CHARACTERS: usize = 160;
    if file_name.chars().count() <= MAX_FILE_NAME_CHARACTERS {
        return file_name.to_string();
    }
    let suffix = file_name
        .rsplit_once('.')
        .filter(|(stem, extension)| {
            !stem.is_empty() && !extension.is_empty() && extension.chars().count() <= 16
        })
        .map(|(_, extension)| format!(".{extension}"))
        .unwrap_or_default();
    let stem_characters = MAX_FILE_NAME_CHARACTERS.saturating_sub(suffix.chars().count());
    format!(
        "{}{suffix}",
        file_name.chars().take(stem_characters).collect::<String>()
    )
}

fn is_windows_reserved_file_name(file_name: &str) -> bool {
    let stem = file_name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

#[cfg(test)]
#[path = "file_upload_tests.rs"]
mod tests;
