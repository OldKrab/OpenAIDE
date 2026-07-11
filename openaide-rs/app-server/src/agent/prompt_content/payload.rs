use crate::protocol::model::Attachment;

pub(super) fn attachment_payload(attachment: &Attachment) -> Option<&serde_json::Value> {
    attachment.payload.as_ref().filter(|value| !value.is_null())
}

pub(super) fn payload_text(payload: &serde_json::Value) -> Option<String> {
    payload.as_str().map(ToString::to_string).or_else(|| {
        payload
            .as_object()
            .and_then(|object| object.get("text"))
            .and_then(|text| text.as_str())
            .map(ToString::to_string)
    })
}

pub(super) fn payload_data(payload: &serde_json::Value) -> Option<String> {
    payload.as_object().and_then(|object| {
        object
            .get("data")
            .or_else(|| object.get("blob"))
            .and_then(|data| data.as_str())
            .map(ToString::to_string)
    })
}

pub(super) fn payload_mime_type(payload: &serde_json::Value) -> Option<String> {
    payload.as_object().and_then(|object| {
        object
            .get("mimeType")
            .or_else(|| object.get("mime_type"))
            .or_else(|| object.get("mime"))
            .and_then(|mime| mime.as_str())
            .filter(|mime| !mime.trim().is_empty())
            .map(ToString::to_string)
    })
}

pub(super) fn is_image_attachment(attachment: &Attachment, mime_type: Option<&str>) -> bool {
    attachment.kind.eq_ignore_ascii_case("image")
        || mime_type.is_some_and(|mime_type| mime_type.starts_with("image/"))
}

pub(super) fn is_audio_attachment(attachment: &Attachment, mime_type: Option<&str>) -> bool {
    attachment.kind.eq_ignore_ascii_case("audio")
        || mime_type.is_some_and(|mime_type| mime_type.starts_with("audio/"))
}
