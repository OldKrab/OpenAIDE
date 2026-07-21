use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::task::ComposerMessage;
use uuid::Uuid;

use crate::attachment_runtime::AttachmentRuntimeError;
use crate::protocol::model::{ChatMessage, NormalizedMessage};
use crate::storage::records::{TaskTitle, TaskTitleSource};
use crate::tasks::lifecycle::running_turn_message;
use crate::tasks::mutation::TaskMutationContext;

use super::{validation_error, TaskProductApi};

impl TaskProductApi {
    pub(super) fn append_user_message(
        &self,
        ctx: &mut TaskMutationContext<'_>,
        identity: &str,
        message_id: &str,
        text: String,
        attachments: Vec<crate::protocol::model::Attachment>,
        created_at: &str,
    ) -> Result<(), crate::protocol::errors::RuntimeError> {
        ctx.append_chat_message(ChatMessage {
            cursor: String::new(),
            identity: identity.to_string(),
            message_type: "user".to_string(),
            message_id: message_id.to_string(),
            message: NormalizedMessage::User {
                id: identity.to_string(),
                text,
                created_at: created_at.to_string(),
                attachments,
            },
        });
        Ok(())
    }

    pub(super) fn append_running_turn(
        &self,
        ctx: &mut TaskMutationContext<'_>,
        task_id: &str,
        turn_id: &str,
        created_at: &str,
    ) -> Result<(), crate::protocol::errors::RuntimeError> {
        let mut message = running_turn_message(created_at);
        let NormalizedMessage::Activity { id, .. } = &mut message else {
            return Err(crate::protocol::errors::RuntimeError::Internal(
                "running turn marker must be an activity".to_string(),
            ));
        };
        *id = format!("turn:{turn_id}");
        let _ = task_id;
        ctx.append_chat_message(ChatMessage {
            cursor: String::new(),
            identity: format!("turn:{turn_id}"),
            message_type: "activity".to_string(),
            message_id: format!("message_{}", Uuid::new_v4()),
            message,
        });
        Ok(())
    }
}

pub(super) fn normalized_message_text(message: &ComposerMessage) -> String {
    message.text.as_deref().unwrap_or("").trim().to_string()
}

/// Creates the provisional single-line title shown until the Agent supplies its own title.
pub(super) fn prompt_title(prompt: &str) -> Option<TaskTitle> {
    const MAX_CHARS: usize = 60;

    let prompt = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let prefix = prompt.chars().take(MAX_CHARS).collect::<String>();
    let value = if prompt.chars().count() > MAX_CHARS {
        format!("{prefix}...")
    } else {
        prefix
    };
    TaskTitle::new(value, TaskTitleSource::Prompt)
}

pub(super) fn protocol_error_from_attachment_runtime(
    error: AttachmentRuntimeError,
) -> ProtocolError {
    match error {
        AttachmentRuntimeError::UnknownHandle => attachment_handle_invalid_error(),
        AttachmentRuntimeError::WrongTask => {
            validation_error("attachments", "Attachment handle belongs to another Task")
        }
        AttachmentRuntimeError::DuplicateHandle => validation_error(
            "attachments",
            "Duplicate attachment handles are not allowed",
        ),
        AttachmentRuntimeError::InvalidImage => {
            validation_error("message.images", "Image data is invalid")
        }
        AttachmentRuntimeError::TooLarge => {
            validation_error("message.images", "Image data is too large")
        }
        AttachmentRuntimeError::InvalidRoot
        | AttachmentRuntimeError::OutsideAllowedRoot
        | AttachmentRuntimeError::UnknownEntry
        | AttachmentRuntimeError::NotDirectory
        | AttachmentRuntimeError::NotFile
        | AttachmentRuntimeError::NotText
        | AttachmentRuntimeError::ReadFailed(_) => {
            validation_error("attachments", "Attachment handle is not sendable")
        }
    }
}

fn attachment_handle_invalid_error() -> ProtocolError {
    let mut error = validation_error(
        "attachments",
        "Attachment is no longer available. Reselect it and try again.",
    );
    error.code = ProtocolErrorCode::AttachmentHandleInvalid;
    error.recoverable = true;
    error
}
