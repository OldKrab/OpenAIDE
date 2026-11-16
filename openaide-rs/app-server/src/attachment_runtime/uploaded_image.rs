//! Composer Images that arrive through the upload route instead of inline in `task/send`.
//!
//! The upload streams the bytes into the App Server's private upload area; this module owns
//! that file from registration until its handle is released, consumed by Send, or expires.
//! Send reads it once and produces the same inline image attachment a pasted Image produces,
//! so Chat storage, Queue capture, and Agent delivery need no second path. No size limit is
//! applied here: the bytes never travel in a protocol message, and the Agent enforces its own.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;

use crate::protocol::model::Attachment;

use super::AttachmentRuntimeError;

/// Header bytes enough for every supported format's signature.
const SIGNATURE_BYTES: usize = 64;

/// The upload file behind one Image handle; dropping the last owner deletes it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UploadedImageFile {
    path: PathBuf,
}

impl Drop for UploadedImageFile {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => crate::logging::warn(
                "attachment_uploaded_image_cleanup_failed",
                serde_json::json!({ "error_kind": format!("{:?}", error.kind()) }),
            ),
        }
    }
}

/// An uploaded file accepted as an Image: its owner, media type, and byte size.
#[derive(Debug)]
pub(crate) struct AcceptedUploadedImage {
    pub(crate) file: Arc<UploadedImageFile>,
    pub(crate) mime_type: &'static str,
    pub(crate) size_bytes: u64,
}

/// Accepts an upload as an Image when its bytes start with a PNG, JPEG, GIF, or WebP signature.
///
/// The media type comes from the bytes, never from the client's file name or declared type.
/// On rejection the caller still owns the file and removes it with the failed upload.
pub(crate) fn accept_uploaded_image(
    path: impl Into<PathBuf>,
) -> Result<AcceptedUploadedImage, AttachmentRuntimeError> {
    let path = path.into();
    let size_bytes = std::fs::metadata(&path)
        .map_err(|error| AttachmentRuntimeError::ReadFailed(error.to_string()))?
        .len();
    if size_bytes == 0 {
        return Err(AttachmentRuntimeError::InvalidImage);
    }
    let mime_type = image_mime_type(&path)?;
    Ok(AcceptedUploadedImage {
        file: Arc::new(UploadedImageFile { path }),
        mime_type,
        size_bytes,
    })
}

fn image_mime_type(path: &Path) -> Result<&'static str, AttachmentRuntimeError> {
    let mut signature = Vec::with_capacity(SIGNATURE_BYTES);
    std::fs::File::open(path)
        .and_then(|file| {
            file.take(SIGNATURE_BYTES as u64)
                .read_to_end(&mut signature)
        })
        .map_err(|error| AttachmentRuntimeError::ReadFailed(error.to_string()))?;
    match image::guess_format(&signature) {
        Ok(image::ImageFormat::Png) => Ok("image/png"),
        Ok(image::ImageFormat::Jpeg) => Ok("image/jpeg"),
        Ok(image::ImageFormat::Gif) => Ok("image/gif"),
        Ok(image::ImageFormat::WebP) => Ok("image/webp"),
        _ => Err(AttachmentRuntimeError::UnsupportedImage),
    }
}

impl UploadedImageFile {
    /// Reads the bytes into the inline image attachment Send stores and delivers.
    ///
    /// A file that vanished (for example, OS temp cleanup) reports an unknown handle, so the
    /// client receives the recoverable handle error and uploads the Image again.
    // TODO: Stream the file into the Task's `images` directory and the Agent prompt instead of
    // holding base64 in memory; that needs Chat storage and ACP delivery to accept a file-backed
    // image, which today take the inline attachment shared with pasted and queued Images.
    pub(crate) fn read_attachment(
        &self,
        label: &str,
        mime_type: &str,
        size_bytes: u64,
    ) -> Result<Attachment, AttachmentRuntimeError> {
        let bytes = std::fs::read(&self.path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AttachmentRuntimeError::UnknownHandle
            } else {
                AttachmentRuntimeError::ReadFailed(error.to_string())
            }
        })?;
        if bytes.len() as u64 != size_bytes {
            return Err(AttachmentRuntimeError::UnknownHandle);
        }
        Ok(Attachment {
            kind: "image".to_string(),
            label: label.to_string(),
            path: None,
            payload: Some(serde_json::json!({
                "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                "mimeType": mime_type,
                "sizeBytes": size_bytes,
            })),
        })
    }
}

#[cfg(test)]
#[path = "uploaded_image_tests.rs"]
mod tests;
