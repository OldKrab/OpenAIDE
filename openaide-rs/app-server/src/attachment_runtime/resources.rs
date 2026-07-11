use std::path::PathBuf;
use std::time::Instant;

use openaide_app_server_protocol::ids::FileBrowserRootId;

use crate::protocol::model::Attachment;

use super::path_validation::AllowedRoot;
use super::{AttachmentOwner, AttachmentRuntimeError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreSendAttachmentHandle {
    pub(super) owner: AttachmentOwner,
    pub(super) label: String,
    pub(super) target: AttachmentTarget,
    pub(super) expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttachmentTarget {
    FileReference {
        path: PathBuf,
        allowed_root: AllowedRoot,
    },
    EmbeddedSnapshot {
        path: PathBuf,
        allowed_root: AllowedRoot,
    },
    PastedImage {
        mime_type: String,
        data: String,
        size_bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddedAttachmentCandidateHandle {
    pub(super) owner: AttachmentOwner,
    pub(super) label: String,
    pub(super) path: PathBuf,
    pub(super) allowed_root: AllowedRoot,
    pub(super) size_bytes: Option<u64>,
    pub(super) expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileBrowserEntryHandle {
    pub(super) owner: AttachmentOwner,
    pub(super) root_id: FileBrowserRootId,
    pub(super) label: String,
    pub(super) path: PathBuf,
    pub(super) allowed_root: AllowedRoot,
    pub(super) kind: openaide_app_server_protocol::attachment::FileBrowserEntryKind,
    pub(super) expires_at: Instant,
}

impl PreSendAttachmentHandle {
    pub(super) fn chat_attachment(&self) -> Attachment {
        if let AttachmentTarget::PastedImage {
            mime_type,
            data,
            size_bytes,
        } = &self.target
        {
            return Attachment {
                kind: self.kind_name().to_string(),
                label: self.label.clone(),
                path: None,
                payload: Some(serde_json::json!({
                    "data": data,
                    "mimeType": mime_type,
                    "sizeBytes": size_bytes,
                })),
            };
        }

        Attachment {
            kind: self.kind_name().to_string(),
            label: self.label.clone(),
            path: None,
            payload: None,
        }
    }

    pub(super) fn agent_attachment(&self) -> Result<Attachment, AttachmentRuntimeError> {
        match &self.target {
            AttachmentTarget::FileReference { path, allowed_root } => {
                allowed_root.validate_file(path)?;
                Ok(Attachment {
                    kind: self.kind_name().to_string(),
                    label: self.label.clone(),
                    path: Some(path.to_string_lossy().to_string()),
                    payload: None,
                })
            }
            AttachmentTarget::EmbeddedSnapshot { path, allowed_root } => {
                allowed_root.validate_file(path)?;
                let text = std::fs::read_to_string(path).map_err(|_| {
                    AttachmentRuntimeError::ReadFailed(
                        "Embedded attachment could not be read".to_string(),
                    )
                })?;
                if text.len() > super::embedded::EMBEDDED_TEXT_MAX_BYTES {
                    return Err(AttachmentRuntimeError::TooLarge);
                }
                Ok(Attachment {
                    kind: self.kind_name().to_string(),
                    label: self.label.clone(),
                    path: None,
                    payload: Some(serde_json::json!({
                        "text": text,
                        "mimeType": "text/plain",
                    })),
                })
            }
            AttachmentTarget::PastedImage {
                mime_type,
                data,
                size_bytes,
            } => Ok(Attachment {
                kind: self.kind_name().to_string(),
                label: self.label.clone(),
                path: None,
                payload: Some(serde_json::json!({
                    "data": data,
                    "mimeType": mime_type,
                    "sizeBytes": size_bytes,
                })),
            }),
        }
    }

    fn kind_name(&self) -> &'static str {
        match &self.target {
            AttachmentTarget::FileReference { .. } => "file_reference",
            AttachmentTarget::EmbeddedSnapshot { .. } => "embedded_snapshot",
            AttachmentTarget::PastedImage { .. } => "image",
        }
    }
}

const PASTED_IMAGE_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub(super) fn validate_pasted_image(
    mime_type: &str,
    data: &str,
) -> Result<u64, AttachmentRuntimeError> {
    if !mime_type.starts_with("image/") || mime_type.trim().len() != mime_type.len() {
        return Err(AttachmentRuntimeError::InvalidImage);
    }
    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
        .map_err(|_| AttachmentRuntimeError::InvalidImage)?;
    if decoded.is_empty() {
        return Err(AttachmentRuntimeError::InvalidImage);
    }
    if decoded.len() as u64 > PASTED_IMAGE_MAX_BYTES {
        return Err(AttachmentRuntimeError::TooLarge);
    }
    Ok(decoded.len() as u64)
}

pub(super) fn safe_image_label(label: String) -> String {
    let trimmed = label.trim();
    let last_segment = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed).trim();
    if last_segment.is_empty() {
        "Pasted image".to_string()
    } else {
        last_segment.chars().take(80).collect()
    }
}
