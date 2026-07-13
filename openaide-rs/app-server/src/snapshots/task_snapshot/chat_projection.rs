use openaide_app_server_protocol::ids::MessageId;
use openaide_app_server_protocol::snapshot::{
    ActivityStatus as ProtocolActivityStatus, ActivityStepSnapshot, AttachmentKind,
    AttachmentSnapshot, ChatItem, ChatItemStatus, ChatRole, MessagePart, PermissionMessageDecision,
    PermissionMessageOption, PermissionMessageOptionKind, PermissionMessageState,
    QuestionMessageAction, QuestionMessageState,
};
use openaide_app_server_protocol::task::{
    ActivityToolContent as ProtocolActivityToolContent,
    ActivityToolField as ProtocolActivityToolField, ActivityToolInput as ProtocolActivityToolInput,
    ActivityToolLocation as ProtocolActivityToolLocation,
    ActivityToolOutput as ProtocolActivityToolOutput, ToolDetailSnapshot,
};

use crate::protocol::model::{
    ActivityStatus, ActivityStep, ActivityToolContent, ActivityToolDetails, AgentContent,
    AgentContentRole, Attachment, ChatMessage, NormalizedMessage, PermissionDecision,
    PermissionOption, PermissionOptionKind, PermissionState, QuestionAction, QuestionState,
};

pub(crate) fn project_chat_item(message: &ChatMessage) -> ChatItem {
    let (role, status, parts) = project_message(&message.message);
    ChatItem {
        message_id: MessageId::from(message.message_id.clone()),
        turn_id: None,
        role,
        status,
        parts,
    }
}

fn project_message(message: &NormalizedMessage) -> (ChatRole, ChatItemStatus, Vec<MessagePart>) {
    match message {
        NormalizedMessage::User {
            text, attachments, ..
        } => {
            // Preserve attachment-only messages without inventing an empty text part.
            let mut parts =
                Vec::with_capacity(usize::from(!text.trim().is_empty()) + attachments.len());
            if !text.trim().is_empty() {
                parts.push(MessagePart::Text { text: text.clone() });
            }
            parts.extend(attachments.iter().enumerate().map(|(index, attachment)| {
                MessagePart::Attachment {
                    attachment: attachment_snapshot(index, attachment),
                }
            }));
            (ChatRole::User, ChatItemStatus::Complete, parts)
        }
        NormalizedMessage::AgentText { text, .. } => (
            ChatRole::Agent,
            ChatItemStatus::Complete,
            vec![MessagePart::Text { text: text.clone() }],
        ),
        NormalizedMessage::Content { role, content, .. } => (
            match role {
                AgentContentRole::Agent => ChatRole::Agent,
                AgentContentRole::Thought => ChatRole::System,
            },
            ChatItemStatus::Complete,
            vec![project_agent_content(content)],
        ),
        NormalizedMessage::Thought { text, .. } => (
            ChatRole::System,
            ChatItemStatus::Complete,
            vec![MessagePart::Text { text: text.clone() }],
        ),
        NormalizedMessage::Activity {
            title,
            status,
            steps,
            ..
        } => (
            ChatRole::System,
            activity_item_status(*status),
            vec![MessagePart::Activity {
                title: title.clone(),
                status: project_activity_status(*status),
                steps: steps.iter().map(project_activity_step).collect(),
            }],
        ),
        NormalizedMessage::Permission {
            request_id,
            app_server_request_id,
            title,
            description,
            scope,
            risk,
            tool_call,
            state,
            options,
            selected_option,
            decision,
            ..
        } => (
            ChatRole::System,
            ChatItemStatus::Complete,
            vec![MessagePart::Permission {
                request_id: request_id.clone().into(),
                app_server_request_id: app_server_request_id.clone().map(Into::into),
                title: title.clone(),
                description: description.clone(),
                scope: scope.clone(),
                risk: risk.clone(),
                tool_call: openaide_app_server_protocol::server_requests::PermissionToolCallRef {
                    id: tool_call.id.clone(),
                    title: tool_call.title.clone(),
                    kind: tool_call.kind.clone(),
                },
                state: project_permission_state(*state),
                options: options.iter().map(project_permission_option).collect(),
                selected_option: selected_option.clone(),
                decision: decision.map(project_permission_decision),
            }],
        ),
        NormalizedMessage::Question {
            request_id,
            message,
            fields,
            state,
            action,
            content,
            error,
            ..
        } => (
            ChatRole::System,
            ChatItemStatus::Complete,
            vec![MessagePart::Question {
                request_id: request_id.clone().into(),
                message: message.clone(),
                fields: fields.clone(),
                state: match state {
                    QuestionState::Pending => QuestionMessageState::Pending,
                    QuestionState::Resolved => QuestionMessageState::Resolved,
                    QuestionState::Cancelled => QuestionMessageState::Cancelled,
                    QuestionState::Error => QuestionMessageState::Error,
                },
                action: action.map(|action| match action {
                    QuestionAction::Submit => QuestionMessageAction::Submit,
                    QuestionAction::Cancel => QuestionMessageAction::Cancel,
                }),
                content: content.clone(),
                error: error.clone(),
            }],
        ),
        NormalizedMessage::Interruption { message, .. } => (
            ChatRole::System,
            ChatItemStatus::Interrupted,
            vec![MessagePart::Text {
                text: message.clone(),
            }],
        ),
    }
}

fn project_agent_content(content: &AgentContent) -> MessagePart {
    match content {
        AgentContent::Image {
            media_type,
            data,
            uri,
        } => MessagePart::Image {
            media_type: media_type.clone(),
            data_url: format!("data:{media_type};base64,{data}"),
            uri: uri.clone(),
        },
        AgentContent::Resource {
            uri,
            name,
            title,
            description,
            media_type,
            size_bytes,
            text,
        } => MessagePart::Resource {
            uri: uri.clone(),
            name: name.clone(),
            title: title.clone(),
            description: description.clone(),
            media_type: media_type.clone(),
            size_bytes: *size_bytes,
            text: text.clone(),
        },
        AgentContent::Unsupported {
            content_type,
            media_type,
            uri,
        } => MessagePart::Unsupported {
            content_type: content_type.clone(),
            media_type: media_type.clone(),
            uri: uri.clone(),
        },
    }
}

fn project_permission_state(state: PermissionState) -> PermissionMessageState {
    match state {
        PermissionState::Pending => PermissionMessageState::Pending,
        PermissionState::Responding => PermissionMessageState::Responding,
        PermissionState::Resolved => PermissionMessageState::Resolved,
        PermissionState::Cancelled => PermissionMessageState::Cancelled,
    }
}

fn project_permission_decision(decision: PermissionDecision) -> PermissionMessageDecision {
    match decision {
        PermissionDecision::Approved => PermissionMessageDecision::Approved,
        PermissionDecision::Denied => PermissionMessageDecision::Denied,
    }
}

fn project_permission_option(option: &PermissionOption) -> PermissionMessageOption {
    PermissionMessageOption {
        option_id: option.id.clone(),
        name: option.label.clone(),
        kind: option.kind.map(project_permission_option_kind),
    }
}

fn project_permission_option_kind(kind: PermissionOptionKind) -> PermissionMessageOptionKind {
    match kind {
        PermissionOptionKind::Allow => PermissionMessageOptionKind::Allow,
        PermissionOptionKind::Deny => PermissionMessageOptionKind::Deny,
        PermissionOptionKind::Other => PermissionMessageOptionKind::Other,
    }
}

fn project_activity_step(step: &ActivityStep) -> ActivityStepSnapshot {
    match step {
        ActivityStep::Text { text, level } => ActivityStepSnapshot::Text {
            text: text.clone(),
            level: level.clone(),
        },
        ActivityStep::Tool {
            tool_call_id,
            name,
            status,
            input_summary,
            output_preview,
            detail_artifact_id,
            details,
        } => ActivityStepSnapshot::Tool {
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            status: project_activity_status(*status),
            input_summary: input_summary.clone(),
            output_preview: output_preview.clone(),
            detail_artifact_id: detail_artifact_id.clone(),
            details: details.as_deref().map(project_tool_details),
        },
        ActivityStep::Command {
            command_label,
            status,
            exit_code,
            output_preview,
        } => ActivityStepSnapshot::Command {
            command_label: command_label.clone(),
            status: project_activity_status(*status),
            exit_code: *exit_code,
            output_preview: output_preview.clone(),
        },
    }
}

pub(crate) fn project_tool_details(details: &ActivityToolDetails) -> ToolDetailSnapshot {
    ToolDetailSnapshot {
        locations: details
            .locations
            .iter()
            .map(|location| ProtocolActivityToolLocation {
                path: location.path.clone(),
                line: location.line,
            })
            .collect(),
        content: details.content.iter().map(project_tool_content).collect(),
        input: details
            .input
            .as_ref()
            .map(|input| ProtocolActivityToolInput {
                command: input.command.clone(),
                cwd: input.cwd.clone(),
                query: input.query.clone(),
                queries: (!input.queries.is_empty()).then(|| input.queries.clone()),
                url: input.url.clone(),
                path: input.path.clone(),
                fields: project_tool_fields(&input.fields),
            }),
        output: details
            .output
            .as_ref()
            .map(|output| ProtocolActivityToolOutput {
                stdout: output.stdout.clone(),
                stderr: output.stderr.clone(),
                formatted_output: output.formatted_output.clone(),
                aggregated_output: output.aggregated_output.clone(),
                exit_code: output.exit_code,
                success: output.success,
                fields: project_tool_fields(&output.fields),
            }),
    }
}

fn project_tool_content(content: &ActivityToolContent) -> ProtocolActivityToolContent {
    match content {
        ActivityToolContent::Text { text } => {
            ProtocolActivityToolContent::Text { text: text.clone() }
        }
        ActivityToolContent::Diff {
            path,
            old_text,
            new_text,
        } => ProtocolActivityToolContent::Diff {
            path: path.clone(),
            old_text: old_text.clone(),
            new_text: new_text.clone(),
        },
        ActivityToolContent::Terminal { terminal_id } => ProtocolActivityToolContent::Terminal {
            terminal_id: terminal_id.clone(),
        },
        ActivityToolContent::Other { label } => ProtocolActivityToolContent::Other {
            label: label.clone(),
        },
    }
}

fn project_tool_fields(
    fields: &[crate::protocol::model::ActivityToolField],
) -> Vec<ProtocolActivityToolField> {
    fields
        .iter()
        .map(|field| ProtocolActivityToolField {
            name: field.name.clone(),
            value: field.value.clone(),
        })
        .collect()
}

fn activity_item_status(status: ActivityStatus) -> ChatItemStatus {
    match status {
        ActivityStatus::Running => ChatItemStatus::Streaming,
        ActivityStatus::Completed => ChatItemStatus::Complete,
        ActivityStatus::Error => ChatItemStatus::Failed,
    }
}

fn project_activity_status(status: ActivityStatus) -> ProtocolActivityStatus {
    match status {
        ActivityStatus::Running => ProtocolActivityStatus::Running,
        ActivityStatus::Completed => ProtocolActivityStatus::Completed,
        ActivityStatus::Error => ProtocolActivityStatus::Failed,
    }
}

fn attachment_snapshot(index: usize, attachment: &Attachment) -> AttachmentSnapshot {
    let media_type = attachment_payload_string(attachment, "mimeType")
        .or_else(|| attachment_payload_string(attachment, "mime"));
    AttachmentSnapshot {
        attachment_id: format!("legacy-attachment-{index}").into(),
        kind: attachment_kind(attachment),
        label: attachment.label.clone(),
        preview_url: attachment_preview_url(attachment, media_type.as_deref()),
        media_type,
        size_bytes: attachment_payload_u64(attachment, "sizeBytes"),
    }
}

fn attachment_kind(attachment: &Attachment) -> AttachmentKind {
    match attachment.kind.as_str() {
        "embedded_snapshot" | "image" => AttachmentKind::EmbeddedSnapshot,
        _ => AttachmentKind::FileReference,
    }
}

fn attachment_preview_url(attachment: &Attachment, media_type: Option<&str>) -> Option<String> {
    let media_type = media_type?;
    if !media_type.starts_with("image/") {
        return None;
    }
    attachment_payload_string(attachment, "data")
        .map(|data| format!("data:{media_type};base64,{data}"))
}

fn attachment_payload_string(attachment: &Attachment, key: &str) -> Option<String> {
    attachment
        .payload
        .as_ref()
        .and_then(|payload| payload.get(key))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn attachment_payload_u64(attachment: &Attachment, key: &str) -> Option<u64> {
    attachment
        .payload
        .as_ref()
        .and_then(|payload| payload.get(key))
        .and_then(|value| value.as_u64())
}

#[cfg(test)]
#[path = "chat_projection_tests.rs"]
mod tests;
