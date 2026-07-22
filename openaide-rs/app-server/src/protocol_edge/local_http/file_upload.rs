use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openaide_app_server_protocol::ids::ClientInstanceId;
use tempfile::NamedTempFile;
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
        temporary: NamedTempFile,
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
    temporary: NamedTempFile,
    last_activity: Instant,
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
                    temporary: temporary_upload(request.file_name)?,
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

pub(crate) fn temporary_upload(file_name: &str) -> Result<NamedTempFile, std::io::Error> {
    tempfile::Builder::new()
        .prefix("openaide-upload-")
        .suffix(&safe_temp_suffix(file_name))
        .tempfile()
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

fn safe_temp_suffix(file_name: &str) -> String {
    let Some(extension) = std::path::Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 16
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
    else {
        return String::new();
    };
    format!(".{extension}")
}

#[cfg(test)]
#[path = "file_upload_tests.rs"]
mod tests;
