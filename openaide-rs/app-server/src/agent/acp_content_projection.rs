use agent_client_protocol::schema::{ContentBlock, EmbeddedResourceResource};

use crate::logging;
use crate::protocol::model::{AgentMessagePart, AgentMessageRole};
use serde_json::json;

/// Normalizes one ACP content block without leaking raw protocol objects into product state.
pub(super) fn project_content_block(
    content: ContentBlock,
    role: AgentMessageRole,
) -> AgentMessagePart {
    let content = match content {
        ContentBlock::Text(text) => AgentMessagePart::Text { text: text.text },
        ContentBlock::Image(image) => {
            if safe_inline_image(&image.mime_type, &image.data) {
                AgentMessagePart::Image {
                    media_type: image.mime_type,
                    data: image.data,
                    uri: image.uri,
                }
            } else {
                AgentMessagePart::Unsupported {
                    content_type: "image".to_string(),
                    media_type: Some(image.mime_type),
                    uri: image.uri,
                }
            }
        }
        ContentBlock::Audio(audio) => AgentMessagePart::Unsupported {
            content_type: "audio".to_string(),
            media_type: Some(audio.mime_type),
            uri: None,
        },
        ContentBlock::Resource(resource) => match resource.resource {
            EmbeddedResourceResource::TextResourceContents(resource) => {
                AgentMessagePart::Resource {
                    uri: resource.uri,
                    name: None,
                    title: None,
                    description: None,
                    media_type: resource.mime_type,
                    size_bytes: None,
                    text: Some(resource.text),
                }
            }
            EmbeddedResourceResource::BlobResourceContents(resource) => {
                AgentMessagePart::Unsupported {
                    content_type: "embedded_binary_resource".to_string(),
                    media_type: resource.mime_type,
                    uri: Some(resource.uri),
                }
            }
            _ => AgentMessagePart::Unsupported {
                content_type: "embedded_resource".to_string(),
                media_type: None,
                uri: None,
            },
        },
        ContentBlock::ResourceLink(resource) => AgentMessagePart::Resource {
            uri: resource.uri,
            name: Some(resource.name),
            title: resource.title,
            description: resource.description,
            media_type: resource.mime_type,
            size_bytes: resource.size.and_then(|size| u64::try_from(size).ok()),
            text: None,
        },
        _ => AgentMessagePart::Unsupported {
            content_type: "unknown".to_string(),
            media_type: None,
            uri: None,
        },
    };
    if let AgentMessagePart::Unsupported {
        content_type,
        media_type,
        ..
    } = &content
    {
        logging::warn(
            "acp_content_preserved_as_unsupported",
            json!({
                "content_type": content_type,
                "media_type": media_type,
                "role": match role {
                    AgentMessageRole::Agent => "agent",
                    AgentMessageRole::Thought => "thought",
                },
            }),
        );
    }
    content
}

const MAX_INLINE_IMAGE_BYTES: usize = 5 * 1024 * 1024;

fn safe_inline_image(media_type: &str, data: &str) -> bool {
    crate::media::validate_base64_image(media_type, data, MAX_INLINE_IMAGE_BYTES).is_ok()
}
