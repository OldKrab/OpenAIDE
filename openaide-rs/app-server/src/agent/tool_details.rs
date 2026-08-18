use crate::agent::acp_schema::{
    ContentBlock, EmbeddedResourceResource, ToolCall, ToolCallContent, ToolCallStatus, ToolKind,
};
use std::ffi::OsStr;

use crate::agent::command_presentation::infer_execute_presentation;
use crate::agent::events::{AgentEvent, AgentToolCall, AgentToolCallStatus};
use crate::agent::tool_details_io::{
    tool_input_detail, tool_input_summary, tool_output_detail, truncate_preview,
};
use crate::protocol::model::{
    ActivityToolContent, ActivityToolDetails, ActivityToolLocation, ToolPresentation,
    ToolPresentationKind,
};
use serde_json::Value;

pub(crate) fn tool_call_event(tool_call: &ToolCall) -> AgentEvent {
    let (kind, input_summary) = tool_presentation(tool_call);
    let presentation = (kind == "execute")
        .then(|| infer_execute_presentation(tool_call.raw_input.as_ref()))
        .flatten()
        .or_else(|| {
            structured_tool_presentation(tool_call.raw_input.as_ref(), input_summary.as_deref())
        });
    AgentEvent::ToolCall(AgentToolCall {
        tool_call_id: tool_call.tool_call_id.to_string(),
        scope_id: None,
        title: tool_call.title.clone(),
        kind,
        status: tool_status(tool_call.status),
        presentation,
        input_summary,
        output_preview: tool_output_preview(tool_call),
        details: tool_details(tool_call),
    })
}

fn tool_presentation(tool_call: &ToolCall) -> (String, Option<String>) {
    let kind = tool_kind_name(tool_call.kind);
    if kind == "search" && is_web_search(tool_call.raw_input.as_ref()) {
        return (
            "web_search".to_string(),
            web_search_input_summary(tool_call.raw_input.as_ref()),
        );
    }
    if kind == "read" {
        if let Some(skill_name) = skill_name_from_locations(tool_call) {
            return ("skill".to_string(), Some(skill_name));
        }
    }
    (kind, tool_input_summary(tool_call.raw_input.as_ref()))
}

fn structured_tool_presentation(
    raw_input: Option<&serde_json::Value>,
    input_summary: Option<&str>,
) -> Option<ToolPresentation> {
    let input = raw_input?.as_object()?;
    let tool_name = input.get("name")?.as_str()?;
    if tool_name != "view_image" {
        return None;
    }
    Some(ToolPresentation::single(
        ToolPresentationKind::View,
        input_summary.map(str::to_string).into_iter().collect(),
    ))
}

fn web_search_input_summary(raw_input: Option<&serde_json::Value>) -> Option<String> {
    let input = raw_input.and_then(serde_json::Value::as_object)?;
    if !input.contains_key("query") && !input.contains_key("q") {
        return None;
    }
    tool_input_summary(raw_input)
}

fn is_web_search(raw_input: Option<&serde_json::Value>) -> bool {
    raw_input
        .and_then(serde_json::Value::as_object)
        .and_then(|input| input.get("type"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|tool_type| tool_type.eq_ignore_ascii_case("webSearch"))
}

fn skill_name_from_locations(tool_call: &ToolCall) -> Option<String> {
    let [location] = tool_call.locations.as_slice() else {
        return None;
    };
    let path = location.path.as_path();
    if path.file_name()? != OsStr::new("SKILL.md") {
        return None;
    }
    let skill_directory = path.parent()?;
    let is_skill_package = skill_directory
        .ancestors()
        .skip(1)
        .any(|ancestor| ancestor.file_name() == Some(OsStr::new("skills")));
    if !is_skill_package {
        return None;
    }
    skill_directory
        .file_name()?
        .to_str()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn tool_content_preview(content: &[ToolCallContent]) -> Option<String> {
    content.iter().find_map(|item| match item {
        ToolCallContent::Content(content) => Some(match &content.content {
            ContentBlock::Text(text) => truncate_preview(text.text.clone()),
            ContentBlock::ResourceLink(_) | ContentBlock::Resource(_) => {
                "Resource output".to_string()
            }
            ContentBlock::Image(_) => "Image output".to_string(),
            ContentBlock::Audio(_) => "Audio output".to_string(),
            _ => "Content output".to_string(),
        }),
        ToolCallContent::Diff(_) => Some("Changed file".to_string()),
        // A terminal item is a reference to separately streamed output, not output itself.
        ToolCallContent::Terminal(_) => None,
        _ => Some("Tool call updated.".to_string()),
    })
}

/// Cursor returns visible read/search/command results in `rawOutput` instead
/// of ACP's typed `content` list. Preserve typed content as the primary source
/// and project only the known scalar result shapes into the compact summary.
fn tool_output_preview(tool_call: &ToolCall) -> Option<String> {
    tool_content_preview(&tool_call.content)
        .or_else(|| raw_output_preview(tool_call.raw_output.as_ref()))
}

fn raw_output_preview(raw_output: Option<&Value>) -> Option<String> {
    let raw_output = raw_output?;
    if let Some(text) = raw_output.as_str().filter(|text| !text.is_empty()) {
        return Some(truncate_preview(text.to_string()));
    }
    let object = raw_output.as_object()?;

    for key in [
        "content",
        "stdout",
        "formattedOutput",
        "formatted_output",
        "aggregatedOutput",
        "aggregated_output",
        "stderr",
    ] {
        if let Some(text) = object
            .get(key)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            return Some(truncate_preview(text.to_string()));
        }
    }

    for (key, singular, plural) in [
        ("totalMatches", "match", "matches"),
        ("totalFiles", "file", "files"),
    ] {
        let Some(count) = object.get(key).and_then(Value::as_u64) else {
            continue;
        };
        let label = if count == 1 { singular } else { plural };
        let suffix = object
            .get("truncated")
            .and_then(Value::as_bool)
            .filter(|truncated| *truncated)
            .map(|_| " (truncated)")
            .unwrap_or_default();
        return Some(format!("{count} {label}{suffix}"));
    }

    object
        .get("exitCode")
        .or_else(|| object.get("exit_code"))
        .and_then(Value::as_i64)
        .map(|exit_code| format!("exit code {exit_code}"))
}

fn tool_details(tool_call: &ToolCall) -> Option<Box<ActivityToolDetails>> {
    let locations = tool_call
        .locations
        .iter()
        .map(|location| ActivityToolLocation {
            path: location.path.display().to_string(),
            line: location.line,
        })
        .collect::<Vec<_>>();
    let content = tool_call
        .content
        .iter()
        .map(tool_content_detail)
        .collect::<Vec<_>>();
    let input = tool_call.raw_input.as_ref().and_then(tool_input_detail);
    let output = tool_call.raw_output.as_ref().and_then(tool_output_detail);
    if locations.is_empty() && content.is_empty() && input.is_none() && output.is_none() {
        None
    } else {
        Some(Box::new(ActivityToolDetails {
            locations,
            content,
            input,
            output,
        }))
    }
}

fn tool_content_detail(content: &ToolCallContent) -> ActivityToolContent {
    match content {
        ToolCallContent::Content(content) => match &content.content {
            ContentBlock::Text(text) => ActivityToolContent::Text {
                text: text.text.clone(),
            },
            ContentBlock::Image(image) => {
                if crate::media::validate_base64_image(
                    &image.mime_type,
                    &image.data,
                    MAX_INLINE_TOOL_MEDIA_BYTES,
                )
                .is_ok()
                {
                    ActivityToolContent::Image {
                        media_type: image.mime_type.clone(),
                        data: image.data.clone(),
                        uri: image.uri.clone(),
                    }
                } else {
                    unsupported_tool_content(
                        "image",
                        Some(image.mime_type.clone()),
                        image.uri.clone(),
                    )
                }
            }
            ContentBlock::Audio(audio) => {
                if crate::media::validate_base64_audio(
                    &audio.mime_type,
                    &audio.data,
                    MAX_INLINE_TOOL_MEDIA_BYTES,
                )
                .is_ok()
                {
                    ActivityToolContent::Audio {
                        media_type: audio.mime_type.clone(),
                        data: audio.data.clone(),
                    }
                } else {
                    unsupported_tool_content("audio", Some(audio.mime_type.clone()), None)
                }
            }
            ContentBlock::ResourceLink(resource) => ActivityToolContent::Resource {
                uri: resource.uri.clone(),
                name: Some(resource.name.clone()),
                title: resource.title.clone(),
                description: resource.description.clone(),
                media_type: resource.mime_type.clone(),
                size_bytes: resource.size.filter(|size| *size >= 0),
                text: None,
            },
            ContentBlock::Resource(resource) => match &resource.resource {
                EmbeddedResourceResource::TextResourceContents(resource) => {
                    ActivityToolContent::Resource {
                        uri: resource.uri.clone(),
                        name: None,
                        title: None,
                        description: None,
                        media_type: resource.mime_type.clone(),
                        size_bytes: None,
                        text: Some(resource.text.clone()),
                    }
                }
                EmbeddedResourceResource::BlobResourceContents(resource) => {
                    ActivityToolContent::Unsupported {
                        content_type: "resource_blob".to_string(),
                        media_type: resource.mime_type.clone(),
                        uri: Some(resource.uri.clone()),
                    }
                }
                _ => unsupported_tool_content("resource", None, None),
            },
            _ => unsupported_tool_content("content", None, None),
        },
        ToolCallContent::Diff(diff) => ActivityToolContent::Diff {
            path: diff.path.display().to_string(),
            old_text: diff.old_text.clone(),
            new_text: diff.new_text.clone(),
        },
        ToolCallContent::Terminal(terminal) => ActivityToolContent::Terminal {
            terminal_id: terminal.terminal_id.to_string(),
        },
        _ => unsupported_tool_content("tool_content", None, None),
    }
}

fn unsupported_tool_content(
    content_type: &str,
    media_type: Option<String>,
    uri: Option<String>,
) -> ActivityToolContent {
    ActivityToolContent::Unsupported {
        content_type: content_type.to_string(),
        media_type,
        uri,
    }
}

const MAX_INLINE_TOOL_MEDIA_BYTES: usize = 10 * 1024 * 1024;

pub(crate) fn tool_kind_name(kind: ToolKind) -> String {
    match kind {
        ToolKind::Read => "read",
        ToolKind::Edit => "edit",
        ToolKind::Delete => "delete",
        ToolKind::Move => "move",
        ToolKind::Search => "search",
        ToolKind::Execute => "execute",
        ToolKind::Think => "think",
        ToolKind::Fetch => "fetch",
        ToolKind::SwitchMode => "switch_mode",
        ToolKind::Other => "other",
        _ => "other",
    }
    .to_string()
}

fn tool_status(status: ToolCallStatus) -> AgentToolCallStatus {
    match status {
        ToolCallStatus::Pending => AgentToolCallStatus::Pending,
        ToolCallStatus::InProgress => AgentToolCallStatus::InProgress,
        ToolCallStatus::Completed => AgentToolCallStatus::Completed,
        ToolCallStatus::Failed => AgentToolCallStatus::Failed,
        _ => AgentToolCallStatus::Pending,
    }
}

#[cfg(test)]
#[path = "tool_details_tests.rs"]
mod tests;
