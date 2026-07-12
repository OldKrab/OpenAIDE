use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{MessageId, TurnId};
use openaide_app_server_protocol::task::ComposerMessage;
use uuid::Uuid;

use crate::attachment_runtime::{AttachmentRuntimeError, ResolvedSendAttachments};
use crate::protocol::model::{ActivityStatus, ChatMessage, NormalizedMessage};
use crate::storage::cursor;
use crate::storage::send_receipts::TaskSendReceipt;
use crate::tasks::lifecycle::running_turn_message;

use super::{conflict_error, storage_error, validation_error, TaskProductApi};

impl TaskProductApi {
    pub(super) fn existing_send(
        &self,
        task_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<ExistingSend>, ProtocolError> {
        let Some(receipt) = self
            .store
            .read_send_receipt(task_id, idempotency_key)
            .map_err(storage_error)?
        else {
            return Ok(None);
        };
        let Some(recovery_required) = self.send_recovery_requirement(task_id, &receipt)? else {
            return Ok(None);
        };
        Ok(Some(ExistingSend {
            fingerprint: SendFingerprint {
                text: receipt.text,
                attachment_handles: receipt.attachment_handles,
            },
            user_message_id: MessageId::from(receipt.user_message_id),
            turn_id: TurnId::from(receipt.turn_id),
            recovery_required,
        }))
    }

    fn send_recovery_requirement(
        &self,
        task_id: &str,
        receipt: &TaskSendReceipt,
    ) -> Result<Option<bool>, ProtocolError> {
        let task = self.store.read_task(task_id).map_err(storage_error)?;
        let messages = self.store.read_messages(task_id).map_err(storage_error)?;
        let expected_identity = send_identity(&receipt.idempotency_key);
        let has_user_message = messages.iter().any(|stored| {
            stored.chat.message_id == receipt.user_message_id
                && stored.chat.identity == expected_identity
                && matches!(
                    &stored.chat.message,
                    NormalizedMessage::User {
                        text,
                        attachments,
                        ..
                    } if text == &receipt.text
                        && attachments.len() == receipt.attachment_handles.len()
                )
        });
        if !has_user_message {
            return Ok(None);
        }
        let expected_turn_identity = format!("turn:{}", receipt.turn_id);
        let turn_status = messages.iter().find_map(|stored| {
            if stored.chat.identity != expected_turn_identity {
                return None;
            }
            match &stored.chat.message {
                NormalizedMessage::Activity { status, .. } => Some(*status),
                _ => None,
            }
        });
        let task_points_to_receipt =
            task.active_turn_id.as_deref() == Some(receipt.turn_id.as_str());
        Ok(Some(match turn_status {
            Some(ActivityStatus::Running) => !task_points_to_receipt,
            Some(_) => task_points_to_receipt,
            None => true,
        }))
    }

    pub(super) fn append_user_message(
        &self,
        task_id: &str,
        identity: &str,
        message_id: &str,
        text: String,
        attachments: Vec<crate::protocol::model::Attachment>,
        created_at: &str,
    ) -> Result<(), crate::protocol::errors::RuntimeError> {
        self.append_chat_message(
            task_id,
            ChatMessage {
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
            },
        )
    }

    pub(super) fn append_running_turn(
        &self,
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
        self.append_chat_message(
            task_id,
            ChatMessage {
                cursor: String::new(),
                identity: format!("turn:{turn_id}"),
                message_type: "activity".to_string(),
                message_id: format!("message_{}", Uuid::new_v4()),
                message,
            },
        )
    }

    fn append_chat_message(
        &self,
        task_id: &str,
        mut message: ChatMessage,
    ) -> Result<(), crate::protocol::errors::RuntimeError> {
        let next_sequence = self
            .store
            .read_messages(task_id)?
            .last()
            .map(|message| message.sequence + 1)
            .unwrap_or(1);
        message.cursor = cursor::from_sequence(next_sequence);
        self.store.append_message(task_id, message)?;
        Ok(())
    }
}

pub(super) struct SendFingerprint {
    pub(super) text: String,
    pub(super) attachment_handles: Vec<String>,
}

impl SendFingerprint {
    pub(super) fn from_request_message(message: &ComposerMessage) -> Self {
        Self {
            text: normalized_message_text(message),
            attachment_handles: message
                .attachments
                .iter()
                .map(|handle| handle.as_str().to_string())
                .collect(),
        }
    }

    pub(super) fn from_message(
        message: &ComposerMessage,
        attachments: &ResolvedSendAttachments,
    ) -> Result<Self, ProtocolError> {
        let text = normalized_message_text(message);
        if text.is_empty() && attachments.fingerprint_handles().is_empty() {
            return Err(validation_error("message.text", "Message text is required"));
        }
        Ok(Self {
            text,
            attachment_handles: attachments.fingerprint_handles(),
        })
    }
}

fn normalized_message_text(message: &ComposerMessage) -> String {
    message.text.as_deref().unwrap_or("").trim().to_string()
}

pub(super) struct ExistingSend {
    pub(super) fingerprint: SendFingerprint,
    pub(super) user_message_id: MessageId,
    pub(super) turn_id: TurnId,
    pub(super) recovery_required: bool,
}

impl ExistingSend {
    pub(super) fn validate(&self, expected: &SendFingerprint) -> Result<(), ProtocolError> {
        if self.fingerprint.text == expected.text
            && self.fingerprint.attachment_handles == expected.attachment_handles
        {
            Ok(())
        } else {
            Err(conflict_error(
                "Idempotency key was already used with different message content",
            ))
        }
    }
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
        AttachmentRuntimeError::InvalidRoot
        | AttachmentRuntimeError::OutsideAllowedRoot
        | AttachmentRuntimeError::UnknownEntry
        | AttachmentRuntimeError::NotDirectory
        | AttachmentRuntimeError::NotFile
        | AttachmentRuntimeError::NotText
        | AttachmentRuntimeError::InvalidImage
        | AttachmentRuntimeError::TooLarge
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

pub(super) fn send_identity(idempotency_key: &str) -> String {
    format!("send:{idempotency_key}")
}

pub(super) fn title_from_prompt(prompt: &str) -> String {
    let prompt = prompt.trim();
    let title: String = prompt.chars().take(60).collect();
    if title.is_empty() {
        return "Untitled task".to_string();
    }
    if prompt.chars().count() > 60 {
        format!("{title}...")
    } else {
        title
    }
}
