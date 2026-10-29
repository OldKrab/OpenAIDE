//! Chat images stored once per Task by content hash (ADR-0075).
//!
//! Chat keeps only a reference: the image id, media type, and size. The bytes live
//! in `<task dir>/images/<sha256>`, shared by the Task's Chat and Subagent
//! histories, so Task delete, retention cleanup, and Reset task history remove
//! them with the Task directory. A file is synced before the Chat write that
//! references it, so a crash leaves at most an unreferenced file, never a
//! reference without bytes.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentMessagePart, Attachment, NormalizedMessage};
use crate::storage::records::StoredMessage;

pub(crate) const IMAGES_DIR: &str = "images";

/// An unreferenced file younger than this may belong to a write whose Chat frame
/// has not committed yet, or to a Subagent history written concurrently, so the
/// sweep leaves it alone.
const UNREFERENCED_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

/// True for the lowercase hex SHA-256 this module produces. Every read checks it,
/// so a client-supplied id can never name a path outside the images directory.
pub(crate) fn is_image_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Writes `bytes` once and returns their id. An existing file is kept but its
/// modification time is refreshed, so the unreferenced sweep cannot remove it
/// between this call and the commit of the Chat frame that references it.
pub(crate) fn store(task_dir: &Path, bytes: &[u8]) -> Result<String, RuntimeError> {
    let image_id = hex(&Sha256::digest(bytes));
    let path = task_dir.join(IMAGES_DIR).join(&image_id);
    match fs::metadata(&path) {
        Ok(metadata) if metadata.len() == bytes.len() as u64 => {
            fs::File::options()
                .write(true)
                .open(&path)?
                .set_modified(SystemTime::now())?;
        }
        _ => crate::storage::atomic::write_bytes(&path, bytes)?,
    }
    Ok(image_id)
}

impl crate::storage::Store {
    /// Reads one stored image of a Task, or `None` when that Task has no such image.
    pub fn read_chat_image(
        &self,
        task_id: &str,
        image_id: &str,
    ) -> Result<Option<Vec<u8>>, RuntimeError> {
        crate::storage::id::validate_task_id(task_id)?;
        read(
            &self
                .root()
                .join("task-store-v1")
                .join("tasks")
                .join(task_id),
            image_id,
        )
    }
}

/// Reads one image of the Task, or `None` when it is not stored.
pub(crate) fn read(task_dir: &Path, image_id: &str) -> Result<Option<Vec<u8>>, RuntimeError> {
    if !is_image_id(image_id) {
        return Ok(None);
    }
    match fs::read(task_dir.join(IMAGES_DIR).join(image_id)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Moves every inline image in `messages` into the Task's image files and leaves
/// references in its place. Returns how many images moved; zero means `messages`
/// is unchanged.
pub(crate) fn externalize_messages(
    task_dir: &Path,
    messages: &mut [StoredMessage],
) -> Result<usize, RuntimeError> {
    let mut moved = 0;
    for stored in messages {
        moved += externalize_message(task_dir, &mut stored.chat.message)?;
    }
    Ok(moved)
}

pub(crate) fn externalize_message(
    task_dir: &Path,
    message: &mut NormalizedMessage,
) -> Result<usize, RuntimeError> {
    match message {
        NormalizedMessage::User {
            text, attachments, ..
        } => {
            move_legacy_text_images(text, attachments);
            let mut moved = 0;
            for attachment in attachments {
                moved += usize::from(externalize_attachment(task_dir, attachment)?);
            }
            Ok(moved)
        }
        NormalizedMessage::AgentMessage { parts, .. } => {
            let mut moved = 0;
            for part in parts {
                let AgentMessagePart::Image {
                    data,
                    image_id,
                    size_bytes,
                    width,
                    height,
                    ..
                } = part
                else {
                    continue;
                };
                if data.is_empty() {
                    continue;
                }
                let bytes = decode(data)?;
                let reference = describe(store(task_dir, &bytes)?, &bytes);
                *image_id = Some(reference.image_id);
                *size_bytes = Some(reference.size_bytes);
                (*width, *height) = split(reference.dimensions);
                data.clear();
                moved += 1;
            }
            Ok(moved)
        }
        _ => Ok(0),
    }
}

/// True when any message still carries image bytes inline.
pub(crate) fn has_inline_images(messages: &[StoredMessage]) -> bool {
    messages.iter().any(|stored| match &stored.chat.message {
        NormalizedMessage::User {
            text, attachments, ..
        } => {
            legacy_text_image_regex().is_match(text)
                || attachments
                    .iter()
                    .any(|attachment| inline_image_payload(attachment).is_some())
        }
        NormalizedMessage::AgentMessage { parts, .. } => parts
            .iter()
            .any(|part| matches!(part, AgentMessagePart::Image { data, .. } if !data.is_empty())),
        _ => false,
    })
}

/// Collects the image ids `messages` reference.
pub(crate) fn referenced_ids<'a>(
    messages: impl IntoIterator<Item = &'a StoredMessage>,
    into: &mut HashSet<String>,
) {
    for stored in messages {
        match &stored.chat.message {
            NormalizedMessage::User { attachments, .. } => {
                into.extend(attachments.iter().filter_map(attachment_image_id));
            }
            NormalizedMessage::AgentMessage { parts, .. } => {
                into.extend(parts.iter().filter_map(|part| match part {
                    AgentMessagePart::Image { image_id, .. } => image_id.clone(),
                    _ => None,
                }));
            }
            _ => {}
        }
    }
}

/// Removes image files older than the grace period that neither
/// `chat_referenced` nor `also_referenced` names. `also_referenced` receives only
/// those old files Chat does not reference, so other histories are read only when
/// such a file exists. Returns how many files were removed.
pub(crate) fn remove_unreferenced(
    task_dir: &Path,
    chat_referenced: &HashSet<String>,
    also_referenced: impl FnOnce(&[String]) -> Result<HashSet<String>, RuntimeError>,
) -> Result<usize, RuntimeError> {
    let entries = match fs::read_dir(task_dir.join(IMAGES_DIR)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let now = SystemTime::now();
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_image_id(name) || chat_referenced.contains(name) {
            continue;
        }
        let age = entry
            .metadata()?
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok());
        if age.is_some_and(|age| age >= UNREFERENCED_GRACE) {
            candidates.push(name.to_string());
        }
    }
    if candidates.is_empty() {
        return Ok(0);
    }
    let kept = also_referenced(&candidates)?;
    let mut removed = 0;
    for image_id in candidates.iter().filter(|id| !kept.contains(*id)) {
        match fs::remove_file(task_dir.join(IMAGES_DIR).join(image_id)) {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(removed)
}

/// Returns which of `image_ids` occur anywhere in `bytes`. Ids are hex, so a
/// match can only be a reference; a false match merely keeps a file.
pub(crate) fn ids_mentioned_in(bytes: &[u8], image_ids: &[String], into: &mut HashSet<String>) {
    let pending = image_ids
        .iter()
        .filter(|id| is_image_id(id) && !into.contains(*id))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if pending.is_empty() {
        return;
    }
    let pattern =
        regex::bytes::Regex::new(&pending.join("|")).expect("hex image ids form a valid pattern");
    for found in pattern.find_iter(bytes) {
        if let Ok(id) = std::str::from_utf8(found.as_bytes()) {
            into.insert(id.to_string());
        }
    }
}

/// The image id of a stored image attachment reference.
pub(crate) fn attachment_image_id(attachment: &Attachment) -> Option<String> {
    attachment
        .payload
        .as_ref()?
        .get("imageId")?
        .as_str()
        .filter(|id| is_image_id(id))
        .map(ToString::to_string)
}

fn externalize_attachment(
    task_dir: &Path,
    attachment: &mut Attachment,
) -> Result<bool, RuntimeError> {
    let Some(data) = inline_image_payload(attachment) else {
        return Ok(false);
    };
    let bytes = decode(&data)?;
    let reference = describe(store(task_dir, &bytes)?, &bytes);
    let payload = attachment
        .payload
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
        .expect("inline image payload is an object");
    payload.remove("data");
    payload.insert("imageId".to_string(), reference.image_id.into());
    payload.insert("sizeBytes".to_string(), reference.size_bytes.into());
    insert_dimensions(payload, reference.dimensions);
    Ok(true)
}

fn insert_dimensions(
    payload: &mut serde_json::Map<String, serde_json::Value>,
    dimensions: Option<(u32, u32)>,
) {
    if let Some((width, height)) = dimensions {
        payload.insert("width".to_string(), width.into());
        payload.insert("height".to_string(), height.into());
    }
}

/// What Chat keeps of a stored image besides its media type (ADR-0075).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageReference {
    pub(crate) image_id: String,
    pub(crate) size_bytes: u64,
    /// Display width and height; `None` when the header is unreadable.
    pub(crate) dimensions: Option<(u32, u32)>,
}

fn describe(image_id: String, bytes: &[u8]) -> ImageReference {
    ImageReference {
        image_id,
        size_bytes: bytes.len() as u64,
        dimensions: crate::media::display_dimensions(std::io::Cursor::new(bytes)),
    }
}

fn split(dimensions: Option<(u32, u32)>) -> (Option<u32>, Option<u32>) {
    dimensions.map_or((None, None), |(width, height)| (Some(width), Some(height)))
}

/// The display size recorded in an image attachment reference.
pub(crate) fn attachment_dimensions(attachment: &Attachment) -> Option<(u32, u32)> {
    let payload = attachment.payload.as_ref()?;
    let read = |key| {
        payload
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    };
    Some((read("width")?, read("height")?))
}

/// Records the display size of stored references written before App Server
/// kept it, reading only each file's header. A reference whose file is missing
/// or unreadable stays as it is. Returns how many references changed.
pub(crate) fn add_missing_dimensions(task_dir: &Path, messages: &mut [StoredMessage]) -> usize {
    let mut read = std::collections::HashMap::<String, Option<(u32, u32)>>::new();
    let mut lookup = |image_id: &str| {
        *read
            .entry(image_id.to_string())
            .or_insert_with(|| stored_dimensions(task_dir, image_id))
    };
    let mut changed = 0;
    for stored in messages {
        match &mut stored.chat.message {
            NormalizedMessage::User { attachments, .. } => {
                for attachment in attachments {
                    let Some(image_id) = attachment_image_id(attachment) else {
                        continue;
                    };
                    if attachment_dimensions(attachment).is_some() {
                        continue;
                    }
                    let Some(dimensions) = lookup(&image_id) else {
                        continue;
                    };
                    if let Some(payload) = attachment
                        .payload
                        .as_mut()
                        .and_then(serde_json::Value::as_object_mut)
                    {
                        insert_dimensions(payload, Some(dimensions));
                        changed += 1;
                    }
                }
            }
            NormalizedMessage::AgentMessage { parts, .. } => {
                for part in parts {
                    let AgentMessagePart::Image {
                        image_id: Some(image_id),
                        width: width @ None,
                        height,
                        ..
                    } = part
                    else {
                        continue;
                    };
                    let Some(dimensions) = lookup(image_id) else {
                        continue;
                    };
                    (*width, *height) = split(Some(dimensions));
                    changed += 1;
                }
            }
            _ => {}
        }
    }
    changed
}

/// True when a stored reference lacks the display size its file can provide.
pub(crate) fn has_references_without_dimensions(messages: &[StoredMessage]) -> bool {
    messages.iter().any(|stored| match &stored.chat.message {
        NormalizedMessage::User { attachments, .. } => attachments.iter().any(|attachment| {
            attachment_image_id(attachment).is_some() && attachment_dimensions(attachment).is_none()
        }),
        NormalizedMessage::AgentMessage { parts, .. } => parts.iter().any(|part| {
            matches!(
                part,
                AgentMessagePart::Image {
                    image_id: Some(_),
                    width: None,
                    ..
                }
            )
        }),
        _ => false,
    })
}

fn stored_dimensions(task_dir: &Path, image_id: &str) -> Option<(u32, u32)> {
    if !is_image_id(image_id) {
        return None;
    }
    let file = fs::File::open(task_dir.join(IMAGES_DIR).join(image_id)).ok()?;
    crate::media::display_dimensions(std::io::BufReader::new(file))
}

/// The reference a draft's inline image will have once stored.
///
/// Mutations publish Chat items drafted before the journal writer stores their
/// images, so projection derives the same content-hash id without touching the
/// Task directory. `None` means the data is not valid base64; the commit that
/// stores it fails, so nothing is published.
pub(crate) fn inline_reference(data: &str) -> Option<ImageReference> {
    let bytes = decode(data).ok()?;
    Some(describe(hex(&Sha256::digest(&bytes)), &bytes))
}

/// The pending reference of an image attachment that still holds its bytes.
pub(crate) fn inline_attachment_reference(attachment: &Attachment) -> Option<ImageReference> {
    inline_reference(&inline_image_payload(attachment)?)
}

/// Returns inline base64 data of an attachment whose media type is an image.
fn inline_image_payload(attachment: &Attachment) -> Option<String> {
    let payload = attachment.payload.as_ref()?.as_object()?;
    let media_type = payload.get("mimeType")?.as_str()?;
    if !media_type.starts_with("image/") {
        return None;
    }
    payload
        .get("data")?
        .as_str()
        .filter(|data| !data.is_empty())
        .map(ToString::to_string)
}

fn legacy_text_image_regex() -> &'static regex::Regex {
    static DATA_IMAGE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)\[@image\]\(data:(image/[a-z0-9.+-]+);base64,([a-z0-9+/=\s]+)\)")
            .expect("legacy data Image regex must compile")
    });
    &DATA_IMAGE
}

/// Moves the retired `[@image](data:...)` text syntax into image attachments,
/// which then externalize like any other. The images follow the remaining text
/// instead of sitting inside it; only messages from early releases used this form.
fn move_legacy_text_images(text: &mut String, attachments: &mut Vec<Attachment>) {
    let regex = legacy_text_image_regex();
    if !regex.is_match(text) {
        return;
    }
    for captures in regex.captures_iter(text) {
        let media_type = captures[1].to_ascii_lowercase();
        let data: String = captures[2]
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        attachments.push(Attachment {
            kind: "image".to_string(),
            label: "Image".to_string(),
            path: None,
            payload: Some(serde_json::json!({ "data": data, "mimeType": media_type })),
        });
    }
    *text = regex.replace_all(text, "").trim().to_string();
}

fn decode(data: &str) -> Result<Vec<u8>, RuntimeError> {
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| RuntimeError::Storage("Stored Chat image is not valid base64".to_string()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "chat_images_tests.rs"]
mod tests;
