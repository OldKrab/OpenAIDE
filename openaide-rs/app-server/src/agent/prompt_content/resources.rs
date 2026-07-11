use agent_client_protocol::schema::{ContentBlock, ResourceLink};

use crate::agent::prompt_content_uri::attachment_resource_name;
use crate::protocol::model::Attachment;

use super::payload::{attachment_payload, payload_mime_type};
use super::PromptContentError;

pub(super) fn resource_link(attachment: &Attachment, uri: String) -> ContentBlock {
    let mut resource = ResourceLink::new(attachment_resource_name(attachment), uri);
    if let Some(payload) = attachment_payload(attachment) {
        if let Some(mime_type) = payload_mime_type(payload) {
            resource = resource.mime_type(mime_type);
        }
    }
    ContentBlock::ResourceLink(resource)
}

pub(super) fn error_for_attachment(attachment: &Attachment, message: &str) -> PromptContentError {
    PromptContentError::new(format!(
        "prompt attachment '{}' {message}",
        attachment_resource_name(attachment)
    ))
}
