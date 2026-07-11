use agent_client_protocol::schema::{ContentBlock, TextContent};
use std::fmt;

use crate::protocol::model::Attachment;

mod attachments;
mod blocks;
mod payload;
mod resources;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PromptContentCapabilities {
    pub(crate) image: bool,
    pub(crate) audio: bool,
    pub(crate) embedded_context: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PromptContentPolicy {
    pub(super) capabilities: PromptContentCapabilities,
    pub(super) host_access: HostAccessPolicy,
}

impl PromptContentPolicy {
    pub(crate) fn new(capabilities: PromptContentCapabilities) -> Self {
        Self {
            capabilities,
            host_access: HostAccessPolicy::FileResourceLinks,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum HostAccessPolicy {
    FileResourceLinks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromptContentError {
    message: String,
}

impl PromptContentError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PromptContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[cfg(test)]
pub(crate) fn build_prompt_content(
    text: String,
    attachments: Vec<Attachment>,
) -> Result<Vec<ContentBlock>, PromptContentError> {
    build_prompt_content_with_policy(
        text,
        attachments,
        PromptContentPolicy::new(PromptContentCapabilities::default()),
    )
}

pub(crate) fn build_prompt_content_with_policy(
    text: String,
    attachments: Vec<Attachment>,
    policy: PromptContentPolicy,
) -> Result<Vec<ContentBlock>, PromptContentError> {
    let mut blocks = Vec::with_capacity(usize::from(!text.trim().is_empty()) + attachments.len());
    if !text.trim().is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(text)));
    }
    for attachment in attachments {
        blocks.push(attachments::attachment_content_block(attachment, policy)?);
    }
    Ok(blocks)
}

pub(crate) fn validate_prompt_attachments(
    attachments: &[Attachment],
    policy: PromptContentPolicy,
) -> Result<(), PromptContentError> {
    for attachment in attachments {
        attachments::attachment_content_block(attachment.clone(), policy)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
