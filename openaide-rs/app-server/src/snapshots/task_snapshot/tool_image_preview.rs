use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use openaide_app_server_protocol::task::ToolImagePreview;

use crate::protocol::model::{ActivityToolDetails, ActivityToolField, ActivityToolValue};
use crate::storage::records::TaskRecord;

const TOOL_IMAGE_PREVIEW_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Loads the first supported structured Tool path without interpreting free-form text.
pub(super) fn load_tool_image_preview(
    task: &TaskRecord,
    details: &ActivityToolDetails,
) -> Option<ToolImagePreview> {
    let workspace_root = Path::new(&task.workspace_root);
    image_candidates(details)
        .into_iter()
        .find_map(|candidate| load_candidate(workspace_root, details, candidate))
}

fn load_candidate(
    workspace_root: &Path,
    details: &ActivityToolDetails,
    candidate: &str,
) -> Option<ToolImagePreview> {
    let selected_path = resolve_candidate_path(workspace_root, details, candidate);
    let resolved_path = std::fs::canonicalize(&selected_path).ok()?;
    let metadata = std::fs::metadata(&resolved_path).ok()?;
    if !metadata.is_file() || metadata.len() > TOOL_IMAGE_PREVIEW_MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(&resolved_path)
        .ok()?
        .take(TOOL_IMAGE_PREVIEW_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() || bytes.len() as u64 > TOOL_IMAGE_PREVIEW_MAX_BYTES {
        return None;
    }
    let media_type = image_media_type(&bytes)?;
    let label = Path::new(candidate)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| "Tool image".to_string());
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(ToolImagePreview {
        label,
        media_type: media_type.to_string(),
        data_url: format!("data:{media_type};base64,{encoded}"),
    })
}

fn image_candidates(details: &ActivityToolDetails) -> Vec<&str> {
    let mut candidates = details
        .locations
        .iter()
        .map(|location| location.path.trim())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    let Some(input) = details.input.as_ref() else {
        return candidates;
    };
    if let Some(path) = input
        .path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        candidates.push(path);
    }
    collect_typed_path_fields(&input.fields, &mut candidates);
    candidates
}

fn collect_typed_path_fields<'a>(fields: &'a [ActivityToolField], candidates: &mut Vec<&'a str>) {
    for field in fields {
        if field.name.eq_ignore_ascii_case("path") {
            if let ActivityToolValue::String { value } = &field.value {
                let path = value.trim();
                if !path.is_empty() {
                    candidates.push(path);
                }
            }
        }
        if let ActivityToolValue::Object { fields } = &field.value {
            collect_typed_path_fields(fields, candidates);
        }
    }
}

fn resolve_candidate_path(
    workspace_root: &Path,
    details: &ActivityToolDetails,
    candidate: &str,
) -> PathBuf {
    let candidate = Path::new(candidate);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }
    let Some(cwd) = details
        .input
        .as_ref()
        .and_then(|input| input.cwd.as_deref())
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
    else {
        return workspace_root.join(candidate);
    };
    let cwd = Path::new(cwd);
    if cwd.is_absolute() {
        cwd.join(candidate)
    } else {
        workspace_root.join(cwd).join(candidate)
    }
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
