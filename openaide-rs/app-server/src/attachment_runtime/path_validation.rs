use std::path::{Path, PathBuf};

use super::AttachmentRuntimeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AllowedRoot {
    selected_root: PathBuf,
    resolved_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ValidatedPathKind {
    Directory,
    File { size_bytes: u64 },
}

impl AllowedRoot {
    pub(super) fn new(path: &Path) -> Result<Self, AttachmentRuntimeError> {
        let selected_root = lexical_absolute(path)?;
        let resolved_root = canonicalize(path, "Allowed root could not be resolved")?;
        if !metadata(&resolved_root)?.is_dir() {
            return Err(AttachmentRuntimeError::NotDirectory);
        }
        Ok(Self {
            selected_root,
            resolved_root,
        })
    }

    pub(super) fn classify(
        &self,
        selected_path: &Path,
    ) -> Result<ValidatedPathKind, AttachmentRuntimeError> {
        let selected_path = lexical_absolute(selected_path)?;
        if !selected_path.starts_with(&self.selected_root) {
            return Err(AttachmentRuntimeError::OutsideAllowedRoot);
        }

        let resolved_path = canonicalize(&selected_path, "Attachment path could not be resolved")?;
        if !resolved_path.starts_with(&self.resolved_root) {
            return Err(AttachmentRuntimeError::OutsideAllowedRoot);
        }

        let metadata = metadata(&resolved_path)?;
        if metadata.is_dir() {
            Ok(ValidatedPathKind::Directory)
        } else if metadata.is_file() {
            Ok(ValidatedPathKind::File {
                size_bytes: metadata.len(),
            })
        } else {
            Err(AttachmentRuntimeError::NotFile)
        }
    }

    pub(super) fn validate_directory(
        &self,
        selected_path: &Path,
    ) -> Result<(), AttachmentRuntimeError> {
        match self.classify(selected_path)? {
            ValidatedPathKind::Directory => Ok(()),
            ValidatedPathKind::File { .. } => Err(AttachmentRuntimeError::NotDirectory),
        }
    }

    pub(super) fn validate_file(
        &self,
        selected_path: &Path,
    ) -> Result<u64, AttachmentRuntimeError> {
        match self.classify(selected_path)? {
            ValidatedPathKind::File { size_bytes } => Ok(size_bytes),
            ValidatedPathKind::Directory => Err(AttachmentRuntimeError::NotFile),
        }
    }
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, AttachmentRuntimeError> {
    std::path::absolute(path)
        .map(|path| normalize_lexically(&path))
        .map_err(|_| {
            AttachmentRuntimeError::ReadFailed(
                "Attachment path could not be normalized".to_string(),
            )
        })
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn canonicalize(path: &Path, message: &str) -> Result<PathBuf, AttachmentRuntimeError> {
    std::fs::canonicalize(path).map_err(|_| AttachmentRuntimeError::ReadFailed(message.to_string()))
}

fn metadata(path: &Path) -> Result<std::fs::Metadata, AttachmentRuntimeError> {
    std::fs::metadata(path).map_err(|_| {
        AttachmentRuntimeError::ReadFailed("Attachment metadata is unavailable".to_string())
    })
}
