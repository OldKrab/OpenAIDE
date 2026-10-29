use openaide_app_server_protocol::ids::MessageId;
use openaide_app_server_protocol::snapshot::{
    ActivityStatus as ProtocolActivityStatus, ActivityStepSnapshot, AttachmentKind,
    AttachmentSnapshot, ChatItem, ChatItemStatus, ChatItemTiming, ChatRole,
    CompactionStatusSnapshot, MessagePart, QuestionMessageAction, QuestionMessageState,
    SubagentActivitySnapshot, TimeSpanSnapshot, ToolPermissionDecisionSnapshot,
    ToolPermissionOutcomeSnapshot,
};
use openaide_app_server_protocol::task::{
    ActivityToolContent as ProtocolActivityToolContent,
    ActivityToolField as ProtocolActivityToolField, ActivityToolInput as ProtocolActivityToolInput,
    ActivityToolLocation as ProtocolActivityToolLocation,
    ActivityToolOutput as ProtocolActivityToolOutput,
    ActivityToolValue as ProtocolActivityToolValue, ToolDetailSnapshot,
};

use crate::protocol::model::{
    ActivityStatus, ActivityStep, ActivityToolContent, ActivityToolDetails, ActivityToolValue,
    AgentMessagePart, AgentMessageRole, Attachment, ChatMessage, ChatTiming, CompactionStatus,
    NormalizedMessage, ObservedSpan, QuestionAction, QuestionState,
};
use crate::storage::chat_images::ImageReference;
use openaide_app_server_protocol::snapshot::ImageDimensions;

pub(crate) fn project_chat_item(message: &ChatMessage) -> ChatItem {
    let (role, status, parts) = project_message(&message.message);
    ChatItem {
        message_id: MessageId::from(message.message_id.clone()),
        turn_id: None,
        role,
        status,
        parts,
        timing: project_timing(&message.timing),
    }
}

fn project_timing(timing: &ChatTiming) -> Option<ChatItemTiming> {
    if timing.is_empty() {
        return None;
    }
    let span = |span: &ObservedSpan| TimeSpanSnapshot {
        started_at: span.started_at.clone(),
        ended_at: span.ended_at.clone(),
    };
    Some(ChatItemTiming {
        sent_at: timing.sent_at.clone(),
        run: timing.run.as_ref().map(span),
        closed_turn: timing.closed_turn.as_ref().map(span),
    })
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
            parts.extend(
                attachments
                    .iter()
                    .enumerate()
                    .map(|(index, attachment)| project_user_attachment(index, attachment)),
            );
            (ChatRole::User, ChatItemStatus::Complete, parts)
        }
        NormalizedMessage::AgentMessage { role, parts, .. } => (
            match role {
                AgentMessageRole::Agent => ChatRole::Agent,
                AgentMessageRole::Thought => ChatRole::System,
            },
            ChatItemStatus::Complete,
            parts
                .iter()
                .filter_map(project_agent_message_part)
                .collect(),
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
        NormalizedMessage::CompletedPlan { entries, .. } => (
            ChatRole::System,
            ChatItemStatus::Complete,
            vec![MessagePart::CompletedPlan {
                entries: super::project_agent_plan(crate::protocol::model::AgentPlan {
                    entries: entries.clone(),
                })
                .entries,
            }],
        ),
        NormalizedMessage::ClosedPlan { entries, .. } => (
            ChatRole::System,
            ChatItemStatus::Complete,
            vec![MessagePart::ClosedPlan {
                entries: super::project_agent_plan(crate::protocol::model::AgentPlan {
                    entries: entries.clone(),
                })
                .entries,
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
            resolution_message,
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
                resolution_message: resolution_message.clone(),
            }],
        ),
        NormalizedMessage::Interruption { message, .. } => (
            ChatRole::System,
            ChatItemStatus::Interrupted,
            vec![MessagePart::Text {
                text: message.clone(),
            }],
        ),
        NormalizedMessage::Compaction {
            status,
            summary,
            error,
            ..
        } => (
            ChatRole::System,
            match status {
                CompactionStatus::InProgress => ChatItemStatus::Streaming,
                CompactionStatus::Failed => ChatItemStatus::Failed,
                CompactionStatus::Cancelled => ChatItemStatus::Interrupted,
                CompactionStatus::Completed | CompactionStatus::Unknown => ChatItemStatus::Complete,
            },
            vec![MessagePart::Compaction {
                status: match status {
                    CompactionStatus::InProgress => CompactionStatusSnapshot::InProgress,
                    CompactionStatus::Completed => CompactionStatusSnapshot::Completed,
                    CompactionStatus::Failed => CompactionStatusSnapshot::Failed,
                    CompactionStatus::Cancelled => CompactionStatusSnapshot::Cancelled,
                    CompactionStatus::Unknown => CompactionStatusSnapshot::Unknown,
                },
                summary: summary.clone(),
                error: error.clone(),
            }],
        ),
    }
}

/// Stored Chat holds images as references (ADR-0075); hydration moved any
/// inline bytes out of stored history. A mutation's draft still holds the bytes
/// it received, so it projects the reference the journal writer will store.
fn project_user_attachment(index: usize, attachment: &Attachment) -> MessagePart {
    if attachment.kind == "image" {
        let media_type = attachment_payload_string(attachment, "mimeType");
        let reference = match crate::storage::chat_images::attachment_image_id(attachment) {
            Some(image_id) => Some(ImageReference {
                image_id,
                size_bytes: attachment_payload_u64(attachment, "sizeBytes").unwrap_or_default(),
                dimensions: crate::storage::chat_images::attachment_dimensions(attachment),
            }),
            None => crate::storage::chat_images::inline_attachment_reference(attachment),
        };
        if let (Some(media_type), Some(reference)) = (media_type, reference) {
            return project_image(media_type, reference, None);
        }
    }
    MessagePart::Attachment {
        attachment: attachment_snapshot(index, attachment),
    }
}

/// Returns `None` only for an Image without a stored reference, which the
/// commit and hydration invariants rule out; Chat then omits it rather than
/// naming bytes that do not exist.
fn project_image(
    media_type: String,
    reference: ImageReference,
    uri: Option<String>,
) -> MessagePart {
    MessagePart::Image {
        media_type,
        image_id: reference.image_id,
        size_bytes: reference.size_bytes,
        dimensions: reference
            .dimensions
            .map(|(width, height)| ImageDimensions { width, height }),
        uri,
    }
}

fn project_agent_message_part(content: &AgentMessagePart) -> Option<MessagePart> {
    Some(match content {
        AgentMessagePart::Text { text } => MessagePart::Text { text: text.clone() },
        AgentMessagePart::Image {
            media_type,
            data,
            image_id,
            size_bytes,
            width,
            height,
            uri,
        } => {
            // A draft holds the Agent's bytes until the journal writer stores them.
            let reference = match image_id {
                Some(image_id) => ImageReference {
                    image_id: image_id.clone(),
                    size_bytes: size_bytes.unwrap_or_default(),
                    dimensions: width.zip(*height),
                },
                None => crate::storage::chat_images::inline_reference(data)
                    .filter(|_| !data.is_empty())?,
            };
            project_image(media_type.clone(), reference, uri.clone())
        }
        AgentMessagePart::Resource {
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
        AgentMessagePart::Unsupported {
            content_type,
            media_type,
            uri,
        } => MessagePart::Unsupported {
            content_type: content_type.clone(),
            media_type: media_type.clone(),
            uri: uri.clone(),
        },
    })
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
            presentation,
            description,
            input_summary,
            output_preview,
            detail_artifact_id,
            details,
            permission_outcomes,
            background_outcome,
            pending,
        } => ActivityStepSnapshot::Tool {
            // A settled step was started, whatever the Agent last announced.
            pending: *pending && *status == ActivityStatus::Running,
            background_outcome: background_outcome.map(|outcome| match outcome {
                crate::protocol::model::BackgroundCommandOutcome::Completed => {
                    openaide_app_server_protocol::snapshot::BackgroundCommandOutcome::Completed
                }
                crate::protocol::model::BackgroundCommandOutcome::Failed => {
                    openaide_app_server_protocol::snapshot::BackgroundCommandOutcome::Failed
                }
                crate::protocol::model::BackgroundCommandOutcome::Stopped => {
                    openaide_app_server_protocol::snapshot::BackgroundCommandOutcome::Stopped
                }
            }),
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            status: project_activity_status(*status),
            presentation: presentation.as_ref().map(|presentation| {
                openaide_app_server_protocol::snapshot::ToolPresentationSnapshot {
                    actions: presentation
                        .actions
                        .iter()
                        .map(project_tool_presentation_action)
                        .collect(),
                }
            }),
            description: description.clone(),
            input_summary: input_summary.clone(),
            output_preview: output_preview.clone(),
            detail_artifact_id: detail_artifact_id.clone(),
            details: details.as_deref().map(project_tool_details),
            permission_outcomes: permission_outcomes
                .iter()
                .map(|outcome| ToolPermissionOutcomeSnapshot {
                    request_id: outcome.request_id.clone().into(),
                    decision: match outcome.decision {
                        crate::protocol::model::ToolPermissionDecision::Approved => {
                            ToolPermissionDecisionSnapshot::Approved
                        }
                        crate::protocol::model::ToolPermissionDecision::Rejected => {
                            ToolPermissionDecisionSnapshot::Rejected
                        }
                        crate::protocol::model::ToolPermissionDecision::Cancelled => {
                            ToolPermissionDecisionSnapshot::Cancelled
                        }
                    },
                    option_id: outcome.option_id.clone(),
                    option_label: outcome.option_label.clone(),
                    resolved_at: outcome.resolved_at.clone(),
                })
                .collect(),
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
        ActivityStep::Subagent {
            subagent_id,
            tool_call_id,
            title,
            thread_id,
            raw_path,
            activity,
            name,
            path,
            status,
            events,
        } => ActivityStepSnapshot::Subagent {
            subagent_id: subagent_id.clone().map(Into::into),
            tool_call_id: tool_call_id.clone(),
            title: title.clone(),
            thread_id: thread_id.clone(),
            raw_path: raw_path.clone(),
            activity: activity.clone(),
            name: name.clone(),
            path: path.clone(),
            status: project_activity_status(*status),
            events: events
                .iter()
                .map(|event| match event {
                    crate::protocol::model::SubagentActivity::Delegated => {
                        SubagentActivitySnapshot::Delegated
                    }
                    crate::protocol::model::SubagentActivity::Interacted => {
                        SubagentActivitySnapshot::Interacted
                    }
                    crate::protocol::model::SubagentActivity::Running => {
                        SubagentActivitySnapshot::Running
                    }
                    crate::protocol::model::SubagentActivity::Completed => {
                        SubagentActivitySnapshot::Completed
                    }
                    crate::protocol::model::SubagentActivity::Failed => {
                        SubagentActivitySnapshot::Failed
                    }
                    crate::protocol::model::SubagentActivity::Stopped => {
                        SubagentActivitySnapshot::Stopped
                    }
                })
                .collect(),
        },
    }
}

fn project_tool_presentation_action(
    action: &crate::protocol::model::ToolPresentationAction,
) -> openaide_app_server_protocol::snapshot::ToolPresentationActionSnapshot {
    use crate::protocol::model::{ToolPresentationAction, ToolSearchTarget};
    use openaide_app_server_protocol::snapshot::{
        ToolPresentationActionSnapshot, ToolSearchTargetSnapshot,
    };

    match action {
        ToolPresentationAction::Skill { subjects } => ToolPresentationActionSnapshot::Skill {
            subjects: subjects.clone(),
        },
        ToolPresentationAction::Read { subjects } => ToolPresentationActionSnapshot::Read {
            subjects: subjects.clone(),
        },
        ToolPresentationAction::View { subjects } => ToolPresentationActionSnapshot::View {
            subjects: subjects.clone(),
        },
        ToolPresentationAction::List { subjects } => ToolPresentationActionSnapshot::List {
            subjects: subjects.clone(),
        },
        ToolPresentationAction::Search {
            query,
            scopes,
            target,
        } => ToolPresentationActionSnapshot::Search {
            query: query.clone(),
            scopes: scopes.clone(),
            target: match target {
                ToolSearchTarget::Contents => ToolSearchTargetSnapshot::Contents,
                ToolSearchTarget::Paths => ToolSearchTargetSnapshot::Paths,
            },
        },
    }
}

pub(crate) fn project_tool_details(details: &ActivityToolDetails) -> ToolDetailSnapshot {
    ToolDetailSnapshot {
        revision: 0,
        terminal_outputs: Vec::new(),
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
        ActivityToolContent::Image {
            media_type,
            data,
            uri,
        } => ProtocolActivityToolContent::Image {
            media_type: media_type.clone(),
            data_url: format!("data:{media_type};base64,{data}"),
            uri: uri.clone(),
        },
        ActivityToolContent::Audio { media_type, data } => ProtocolActivityToolContent::Audio {
            media_type: media_type.clone(),
            data_url: format!("data:{media_type};base64,{data}"),
        },
        ActivityToolContent::Resource {
            uri,
            name,
            title,
            description,
            media_type,
            size_bytes,
            text,
        } => ProtocolActivityToolContent::Resource {
            uri: uri.clone(),
            name: name.clone(),
            title: title.clone(),
            description: description.clone(),
            media_type: media_type.clone(),
            size_bytes: *size_bytes,
            text: text.clone(),
        },
        ActivityToolContent::Unsupported {
            content_type,
            media_type,
            uri,
        } => ProtocolActivityToolContent::Unsupported {
            content_type: content_type.clone(),
            media_type: media_type.clone(),
            uri: uri.clone(),
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
            value: project_tool_value(&field.value),
        })
        .collect()
}

fn project_tool_value(value: &ActivityToolValue) -> ProtocolActivityToolValue {
    match value {
        ActivityToolValue::Null => ProtocolActivityToolValue::Null,
        ActivityToolValue::Boolean { value } => {
            ProtocolActivityToolValue::Boolean { value: *value }
        }
        ActivityToolValue::Number { value } => ProtocolActivityToolValue::Number {
            value: value.clone(),
        },
        ActivityToolValue::String { value } => ProtocolActivityToolValue::String {
            value: value.clone(),
        },
        ActivityToolValue::Array { items } => ProtocolActivityToolValue::Array {
            items: items.iter().map(project_tool_value).collect(),
        },
        ActivityToolValue::Object { fields } => ProtocolActivityToolValue::Object {
            fields: project_tool_fields(fields),
        },
        ActivityToolValue::Redacted => ProtocolActivityToolValue::Redacted,
    }
}

fn activity_item_status(status: ActivityStatus) -> ChatItemStatus {
    match status {
        ActivityStatus::Running => ChatItemStatus::Streaming,
        ActivityStatus::Completed => ChatItemStatus::Complete,
        ActivityStatus::Interrupted => ChatItemStatus::Interrupted,
        ActivityStatus::Error => ChatItemStatus::Failed,
    }
}

fn project_activity_status(status: ActivityStatus) -> ProtocolActivityStatus {
    match status {
        ActivityStatus::Running => ProtocolActivityStatus::Running,
        ActivityStatus::Completed => ProtocolActivityStatus::Completed,
        ActivityStatus::Interrupted => ProtocolActivityStatus::Interrupted,
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
        preview_image_id: crate::storage::chat_images::attachment_image_id(attachment),
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
