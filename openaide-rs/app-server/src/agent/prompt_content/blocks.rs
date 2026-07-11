use agent_client_protocol::schema::{
    AudioContent, BlobResourceContents, ContentBlock, EmbeddedResource, EmbeddedResourceResource,
    ImageContent, TextResourceContents,
};

use crate::agent::prompt_content_uri::embedded_attachment_uri;
use crate::protocol::model::Attachment;

use super::payload::{
    is_audio_attachment, is_image_attachment, payload_data, payload_mime_type, payload_text,
};
use super::resources::error_for_attachment;
use super::{PromptContentError, PromptContentPolicy};

pub(super) fn payload_content_block(
    attachment: &Attachment,
    payload: &serde_json::Value,
    uri: Option<String>,
    policy: PromptContentPolicy,
) -> Result<ContentBlock, PromptContentError> {
    let mime_type = payload_mime_type(payload);
    if is_image_attachment(attachment, mime_type.as_deref()) {
        return image_content_block(attachment, payload, uri, mime_type, policy);
    }

    if is_audio_attachment(attachment, mime_type.as_deref()) {
        return audio_content_block(attachment, payload, mime_type, policy);
    }

    embedded_resource_block(attachment, payload, uri, mime_type, policy)
}

fn image_content_block(
    attachment: &Attachment,
    payload: &serde_json::Value,
    uri: Option<String>,
    mime_type: Option<String>,
    policy: PromptContentPolicy,
) -> Result<ContentBlock, PromptContentError> {
    if !policy.capabilities.image {
        return Err(error_for_attachment(
            attachment,
            "requires the Agent image prompt capability",
        ));
    }
    let data = payload_data(payload).ok_or_else(|| {
        error_for_attachment(attachment, "image payload must include base64 data")
    })?;
    let mime_type = mime_type.unwrap_or_else(|| "image/png".to_string());
    let mut image = ImageContent::new(data, mime_type);
    if let Some(uri) = uri {
        image = image.uri(uri);
    }
    Ok(ContentBlock::Image(image))
}

fn audio_content_block(
    attachment: &Attachment,
    payload: &serde_json::Value,
    mime_type: Option<String>,
    policy: PromptContentPolicy,
) -> Result<ContentBlock, PromptContentError> {
    if !policy.capabilities.audio {
        return Err(error_for_attachment(
            attachment,
            "requires the Agent audio prompt capability",
        ));
    }
    let data = payload_data(payload).ok_or_else(|| {
        error_for_attachment(attachment, "audio payload must include base64 data")
    })?;
    let mime_type = mime_type.unwrap_or_else(|| "audio/wav".to_string());
    Ok(ContentBlock::Audio(AudioContent::new(data, mime_type)))
}

fn embedded_resource_block(
    attachment: &Attachment,
    payload: &serde_json::Value,
    uri: Option<String>,
    mime_type: Option<String>,
    policy: PromptContentPolicy,
) -> Result<ContentBlock, PromptContentError> {
    if !policy.capabilities.embedded_context {
        return Err(error_for_attachment(
            attachment,
            "requires the Agent embedded context prompt capability",
        ));
    }

    let uri = uri.unwrap_or_else(|| embedded_attachment_uri(attachment));
    if let Some(text) = payload_text(payload) {
        let mut resource = TextResourceContents::new(text, uri);
        if let Some(mime_type) = mime_type {
            resource = resource.mime_type(mime_type);
        }
        return Ok(ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::TextResourceContents(resource),
        )));
    }

    if let Some(blob) = payload_data(payload) {
        let mut resource = BlobResourceContents::new(blob, uri);
        if let Some(mime_type) = mime_type {
            resource = resource.mime_type(mime_type);
        }
        return Ok(ContentBlock::Resource(EmbeddedResource::new(
            EmbeddedResourceResource::BlobResourceContents(resource),
        )));
    }

    Err(error_for_attachment(
        attachment,
        "payload must include text or base64 data",
    ))
}
