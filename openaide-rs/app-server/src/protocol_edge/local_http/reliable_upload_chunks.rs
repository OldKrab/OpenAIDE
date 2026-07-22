use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;

const MAX_CHUNK_BYTES: usize = 512 * 1024;
const MAX_UPLOAD_BYTES: usize = 128 * 1024 * 1024;
const STALE_UPLOAD_AGE: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Default)]
pub(super) struct ReliableUploadChunkRegistry {
    uploads: Arc<Mutex<HashMap<(String, u64), PendingUpload>>>,
}

struct PendingUpload {
    total_size: usize,
    bytes: Vec<u8>,
    last_activity: Instant,
}

pub(super) enum AppendOutcome {
    Partial,
    Complete(String),
}

#[derive(Debug, Error)]
pub(super) enum AppendError {
    #[error("reliable upload chunk is invalid")]
    InvalidChunk,
    #[error("reliable upload chunk exceeds the size limit")]
    ChunkTooLarge,
    #[error("reliable upload exceeds the size limit")]
    UploadTooLarge,
    #[error("reliable upload metadata changed during transfer")]
    MetadataMismatch,
    #[error("reliable upload chunk offset is invalid")]
    OffsetMismatch,
    #[error("reliable upload is not valid UTF-8")]
    InvalidUtf8,
    #[error("reliable upload state is unavailable")]
    StateUnavailable,
}

impl ReliableUploadChunkRegistry {
    /// Reassembles one authenticated reliable-session frame in memory only.
    pub(super) fn append(
        &self,
        session_id: &str,
        sequence: u64,
        offset: usize,
        total_size: usize,
        chunk: Vec<u8>,
    ) -> Result<AppendOutcome, AppendError> {
        if chunk.is_empty() || offset > total_size || chunk.len() > total_size - offset {
            return Err(AppendError::InvalidChunk);
        }
        if chunk.len() > MAX_CHUNK_BYTES {
            return Err(AppendError::ChunkTooLarge);
        }
        if total_size > MAX_UPLOAD_BYTES {
            return Err(AppendError::UploadTooLarge);
        }
        let key = (session_id.to_string(), sequence);
        let now = Instant::now();
        let mut uploads = self
            .uploads
            .lock()
            .map_err(|_| AppendError::StateUnavailable)?;
        uploads.retain(|_, upload| now.duration_since(upload.last_activity) < STALE_UPLOAD_AGE);
        // Offset zero deliberately restarts a frame after an acknowledgement loss.
        if offset == 0 {
            uploads.insert(
                key.clone(),
                PendingUpload {
                    total_size,
                    bytes: Vec::with_capacity(total_size),
                    last_activity: now,
                },
            );
        }
        let upload = uploads.get_mut(&key).ok_or(AppendError::OffsetMismatch)?;
        if upload.total_size != total_size {
            return Err(AppendError::MetadataMismatch);
        }
        if upload.bytes.len() != offset {
            return Err(AppendError::OffsetMismatch);
        }
        upload.bytes.extend_from_slice(&chunk);
        upload.last_activity = now;
        if upload.bytes.len() < total_size {
            return Ok(AppendOutcome::Partial);
        }
        let upload = uploads.remove(&key).ok_or(AppendError::StateUnavailable)?;
        String::from_utf8(upload.bytes)
            .map(AppendOutcome::Complete)
            .map_err(|_| AppendError::InvalidUtf8)
    }
}
