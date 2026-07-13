use crate::agent::acp_schema::ContentBlock;

use crate::agent::prompt_content_uri::attachment_resource_uri;
use crate::protocol::model::Attachment;

use super::blocks::payload_content_block;
use super::payload::attachment_payload;
use super::resources::{error_for_attachment, resource_link};
use super::{HostAccessPolicy, PromptContentError, PromptContentPolicy};

pub(super) fn attachment_content_block(
    attachment: Attachment,
    policy: PromptContentPolicy,
) -> Result<ContentBlock, PromptContentError> {
    if let Some(uri) = attachment_resource_uri(&attachment) {
        return match attachment_payload(&attachment) {
            Some(payload) => {
                match payload_content_block(&attachment, payload, Some(uri.clone()), policy) {
                    Ok(block) => Ok(block),
                    Err(_) => match policy.host_access {
                        HostAccessPolicy::FileResourceLinks => Ok(resource_link(&attachment, uri)),
                    },
                }
            }
            None => Ok(resource_link(&attachment, uri)),
        };
    }

    if let Some(payload) = attachment_payload(&attachment) {
        return payload_content_block(&attachment, payload, None, policy);
    }

    Err(error_for_attachment(
        &attachment,
        "must reference an absolute file path or carry supported embedded content",
    ))
}
