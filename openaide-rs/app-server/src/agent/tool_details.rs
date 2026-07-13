use crate::agent::acp_schema::{
    ContentBlock, EmbeddedResourceResource, ToolCall, ToolCallContent, ToolCallStatus, ToolKind,
};
use std::ffi::OsStr;

use crate::agent::events::{AgentEvent, AgentToolCall, AgentToolCallStatus};
use crate::agent::tool_details_io::{
    tool_input_detail, tool_input_summary, tool_output_detail, truncate_preview,
};
use crate::protocol::model::{ActivityToolContent, ActivityToolDetails, ActivityToolLocation};

pub(crate) fn tool_call_event(tool_call: &ToolCall) -> AgentEvent {
    let (kind, input_summary) = tool_presentation(tool_call);
    AgentEvent::ToolCall(AgentToolCall {
        tool_call_id: tool_call.tool_call_id.to_string(),
        scope_id: None,
        title: tool_call.title.clone(),
        kind,
        status: tool_status(tool_call.status),
        input_summary,
        output_preview: tool_content_preview(&tool_call.content),
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
    content
        .iter()
        .map(|item| match item {
            ToolCallContent::Content(content) => match &content.content {
                ContentBlock::Text(text) => truncate_preview(text.text.clone()),
                ContentBlock::ResourceLink(_) | ContentBlock::Resource(_) => {
                    "Resource output".to_string()
                }
                ContentBlock::Image(_) => "Image output".to_string(),
                ContentBlock::Audio(_) => "Audio output".to_string(),
                _ => "Content output".to_string(),
            },
            ToolCallContent::Diff(_) => "Changed file".to_string(),
            ToolCallContent::Terminal(_) => "Terminal output".to_string(),
            _ => "Tool call updated.".to_string(),
        })
        .next()
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
