use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::Engine;
use openaide_app_server_protocol::file_viewer::{
    FileViewerError, FileViewerKind, FileViewerSnapshot,
};
use openaide_app_server_protocol::ids::{ClientInstanceId, FileViewerHandleId};
use openaide_app_server_protocol::task::ToolImagePreview;
use uuid::Uuid;

const TEXT_PREFIX_BYTES: u64 = 1024 * 1024;
const IMAGE_MAX_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct FileViewerRegistry {
    inner: Arc<Mutex<FileViewerRegistryInner>>,
}

#[derive(Default)]
struct FileViewerRegistryInner {
    targets: std::collections::HashMap<String, FileViewerTarget>,
}

#[derive(Clone)]
struct FileViewerTarget {
    owner: ClientInstanceId,
    path: PathBuf,
}

impl FileViewerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(
        &self,
        owner: &ClientInstanceId,
        workspace_root: &str,
        path: &str,
        line: Option<u32>,
    ) -> FileViewerSnapshot {
        let display_path = resolve_open_path(workspace_root, path);
        self.snapshot_for_path(owner, display_path, line)
    }

    pub fn open_from_handle(
        &self,
        owner: &ClientInstanceId,
        handle: &FileViewerHandleId,
        href: &str,
    ) -> FileViewerSnapshot {
        let Some(base) = self.target_for(owner, handle) else {
            return missing_handle_snapshot(handle);
        };
        let href = href.trim();
        if href.is_empty() {
            return self.snapshot_for_path(owner, base.path, None);
        }
        if let Some(line) = fragment_line(href) {
            return read_snapshot(handle.clone(), base.path, Some(line));
        }
        let next = resolve_from_handle(&base.path, href);
        self.snapshot_for_path(owner, next, parse_line_suffix(href).1)
    }

    pub fn refresh(
        &self,
        owner: &ClientInstanceId,
        handle: &FileViewerHandleId,
        line: Option<u32>,
    ) -> FileViewerSnapshot {
        let Some(target) = self.target_for(owner, handle) else {
            return missing_handle_snapshot(handle);
        };
        read_snapshot(handle.clone(), target.path, line)
    }

    pub fn release(&self, owner: &ClientInstanceId, handle: &FileViewerHandleId) {
        let mut inner = self.inner.lock().expect("file viewer registry poisoned");
        if inner
            .targets
            .get(handle.as_str())
            .is_some_and(|target| &target.owner == owner)
        {
            inner.targets.remove(handle.as_str());
        }
    }

    pub fn release_client(&self, owner: &ClientInstanceId) {
        let mut inner = self.inner.lock().expect("file viewer registry poisoned");
        inner.targets.retain(|_, target| &target.owner != owner);
    }

    fn target_for(
        &self,
        owner: &ClientInstanceId,
        handle: &FileViewerHandleId,
    ) -> Option<FileViewerTarget> {
        let inner = self.inner.lock().expect("file viewer registry poisoned");
        inner
            .targets
            .get(handle.as_str())
            .filter(|target| &target.owner == owner)
            .cloned()
    }

    fn snapshot_for_path(
        &self,
        owner: &ClientInstanceId,
        path: PathBuf,
        line: Option<u32>,
    ) -> FileViewerSnapshot {
        let handle = FileViewerHandleId::new(format!("file-viewer-{}", Uuid::new_v4()));
        {
            let mut inner = self.inner.lock().expect("file viewer registry poisoned");
            inner.targets.insert(
                handle.as_str().to_string(),
                FileViewerTarget {
                    owner: owner.clone(),
                    path: path.clone(),
                },
            );
        }
        read_snapshot(handle, path, line)
    }
}

fn missing_handle_snapshot(handle: &FileViewerHandleId) -> FileViewerSnapshot {
    FileViewerSnapshot {
        handle: handle.clone(),
        display_path: String::new(),
        basename: String::new(),
        kind: FileViewerKind::Error,
        text: None,
        language: None,
        preview: None,
        truncated: false,
        error: Some(FileViewerError::Unreadable),
        focus_line: None,
    }
}

fn read_snapshot(
    handle: FileViewerHandleId,
    path: PathBuf,
    line: Option<u32>,
) -> FileViewerSnapshot {
    let display_path = path.to_string_lossy().into_owned();
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("File")
        .to_string();
    let language = language_from_basename(&basename);
    let mut snapshot = FileViewerSnapshot {
        handle,
        display_path,
        basename,
        kind: FileViewerKind::Error,
        text: None,
        language,
        preview: None,
        truncated: false,
        error: None,
        focus_line: line,
    };
    match load_path(&path) {
        Ok(loaded) => apply_loaded(&mut snapshot, loaded),
        Err(error) => {
            snapshot.kind = FileViewerKind::Error;
            snapshot.error = Some(error);
        }
    }
    snapshot
}

enum LoadedFile {
    Text {
        text: String,
        truncated: bool,
        markdown: bool,
    },
    Image {
        label: String,
        media_type: &'static str,
        bytes: Vec<u8>,
    },
    Binary,
}

fn load_path(path: &Path) -> Result<LoadedFile, FileViewerError> {
    let metadata = std::fs::metadata(path).map_err(map_io_error)?;
    if metadata.is_dir() || !metadata.is_file() {
        return Err(FileViewerError::NotAFile);
    }
    let mut file = std::fs::File::open(path).map_err(map_io_error)?;
    let mut probe = Vec::new();
    file.by_ref()
        .take(16)
        .read_to_end(&mut probe)
        .map_err(|_| FileViewerError::Unreadable)?;
    if let Some(media_type) = image_media_type(&probe) {
        let mut bytes = probe;
        file.take(IMAGE_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| FileViewerError::Unreadable)?;
        if bytes.is_empty() || bytes.len() as u64 > IMAGE_MAX_BYTES {
            return Err(FileViewerError::Unsupported);
        }
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Image")
            .to_string();
        return Ok(LoadedFile::Image {
            label,
            media_type,
            bytes,
        });
    }
    let mut bytes = probe;
    file.take(TEXT_PREFIX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FileViewerError::Unreadable)?;
    let truncated = bytes.len() as u64 > TEXT_PREFIX_BYTES;
    if truncated {
        bytes.truncate(TEXT_PREFIX_BYTES as usize);
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(LoadedFile::Text {
            markdown: is_markdown_name(path),
            text,
            truncated,
        }),
        Err(_) => Ok(LoadedFile::Binary),
    }
}

fn apply_loaded(snapshot: &mut FileViewerSnapshot, loaded: LoadedFile) {
    match loaded {
        LoadedFile::Text {
            text,
            truncated,
            markdown,
        } => {
            snapshot.kind = if markdown {
                FileViewerKind::Markdown
            } else {
                FileViewerKind::Source
            };
            snapshot.text = Some(text);
            snapshot.truncated = truncated;
        }
        LoadedFile::Image {
            label,
            media_type,
            bytes,
        } => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            snapshot.kind = FileViewerKind::Image;
            snapshot.preview = Some(ToolImagePreview {
                label,
                media_type: media_type.to_string(),
                data_url: format!("data:{media_type};base64,{encoded}"),
            });
        }
        LoadedFile::Binary => {
            snapshot.kind = FileViewerKind::Binary;
            snapshot.error = Some(FileViewerError::Unsupported);
        }
    }
}

fn map_io_error(error: std::io::Error) -> FileViewerError {
    match error.kind() {
        ErrorKind::NotFound => FileViewerError::NotFound,
        ErrorKind::PermissionDenied => FileViewerError::PermissionDenied,
        _ => FileViewerError::Unreadable,
    }
}

fn resolve_open_path(workspace_root: &str, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if is_absolute_file_path(&path) {
        path
    } else {
        Path::new(workspace_root).join(path)
    }
}

fn resolve_from_handle(base_file: &Path, href: &str) -> PathBuf {
    let (without_line, _) = parse_line_suffix(href);
    let linked = Path::new(without_line);
    if is_absolute_file_path(linked) {
        linked.to_path_buf()
    } else {
        base_file
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .join(linked)
    }
}

fn is_absolute_file_path(path: &Path) -> bool {
    path.is_absolute() || {
        let value = path.to_string_lossy();
        value.len() >= 3
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':'
            && (value.as_bytes()[2] == b'/' || value.as_bytes()[2] == b'\\')
    }
}

fn parse_line_suffix(value: &str) -> (&str, Option<u32>) {
    let Some((prefix, suffix)) = value.rsplit_once(':') else {
        return (value, None);
    };
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return (value, None);
    }
    let line = suffix.parse::<u32>().ok().filter(|line| *line > 0);
    if line.is_some() {
        (prefix, line)
    } else {
        (value, None)
    }
}

fn fragment_line(href: &str) -> Option<u32> {
    let fragment = href.strip_prefix('#')?;
    let digits = fragment
        .strip_prefix('L')
        .or_else(|| fragment.strip_prefix('l'))
        .unwrap_or(fragment);
    digits.parse::<u32>().ok().filter(|line| *line > 0)
}

fn is_markdown_name(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md" | "markdown" | "mdown")
    )
}

fn language_from_basename(basename: &str) -> Option<String> {
    let ext = Path::new(basename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)?;
    Some(ext)
}

fn image_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
#[path = "file_viewer_tests.rs"]
mod tests;
