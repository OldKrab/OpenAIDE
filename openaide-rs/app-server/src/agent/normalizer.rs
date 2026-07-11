use uuid::Uuid;

use crate::agent::events::{AgentEvent, AgentPermissionOptionKind, AgentToolCallStatus};
use crate::protocol::model::{
    ActivityStatus, ActivityStep, NormalizedMessage, PermissionOption, PermissionOptionKind,
    PermissionState, PermissionToolCall,
};

pub fn normalize_events(events: Vec<AgentEvent>, created_at: &str) -> Vec<NormalizedMessage> {
    events
        .into_iter()
        .filter_map(|event| match event {
            AgentEvent::Text(text) | AgentEvent::TextChunk { text, .. } => {
                Some(NormalizedMessage::AgentText {
                    id: Uuid::new_v4().to_string(),
                    text,
                    created_at: created_at.to_string(),
                    streaming: false,
                })
            }
            AgentEvent::Thought(text) | AgentEvent::ThoughtChunk { text, .. } => {
                Some(NormalizedMessage::Thought {
                    id: Uuid::new_v4().to_string(),
                    text,
                    created_at: created_at.to_string(),
                    streaming: false,
                })
            }
            AgentEvent::ToolCall(tool_call) => {
                let status = tool_call_status(tool_call.status);
                let id = match tool_call.scope_id {
                    Some(scope_id) => format!("acp_tool:{scope_id}:{}", tool_call.tool_call_id),
                    None => format!("acp_tool:{}", tool_call.tool_call_id),
                };
                Some(NormalizedMessage::Activity {
                    id,
                    title: tool_call.title,
                    status,
                    created_at: created_at.to_string(),
                    collapsed: true,
                    steps: vec![ActivityStep::Tool {
                        tool_call_id: Some(tool_call.tool_call_id),
                        name: tool_call.kind,
                        status,
                        input_summary: tool_call.input_summary,
                        output_preview: tool_call.output_preview,
                        detail_artifact_id: None,
                        details: tool_call.details,
                    }],
                })
            }
            AgentEvent::Activity {
                title,
                tool_name,
                output_preview,
            } => Some(NormalizedMessage::Activity {
                id: Uuid::new_v4().to_string(),
                title,
                status: ActivityStatus::Completed,
                created_at: created_at.to_string(),
                collapsed: true,
                steps: vec![ActivityStep::Tool {
                    tool_call_id: None,
                    name: tool_name,
                    status: ActivityStatus::Completed,
                    input_summary: None,
                    output_preview: Some(output_preview),
                    detail_artifact_id: None,
                    details: None,
                }],
            }),
            AgentEvent::PermissionRequest(request) => Some(NormalizedMessage::Permission {
                id: Uuid::new_v4().to_string(),
                request_id: request.request_id,
                app_server_request_id: None,
                title: request.title,
                description: request.description,
                scope: request.scope,
                risk: request.risk,
                tool_call: PermissionToolCall {
                    id: request.tool_call.tool_call_id,
                    title: request.tool_call.title,
                    kind: request.tool_call.kind,
                },
                state: PermissionState::Pending,
                created_at: created_at.to_string(),
                options: request
                    .options
                    .into_iter()
                    .map(|option| PermissionOption {
                        id: option.option_id,
                        label: option.name,
                        kind: Some(match option.kind {
                            AgentPermissionOptionKind::AllowOnce
                            | AgentPermissionOptionKind::AllowAlways => PermissionOptionKind::Allow,
                            AgentPermissionOptionKind::RejectOnce
                            | AgentPermissionOptionKind::RejectAlways => PermissionOptionKind::Deny,
                        }),
                        description: Some(match option.kind {
                            AgentPermissionOptionKind::AllowOnce => "Only this request".to_string(),
                            AgentPermissionOptionKind::AllowAlways => {
                                "Remember this choice".to_string()
                            }
                            AgentPermissionOptionKind::RejectOnce => {
                                "Deny this request".to_string()
                            }
                            AgentPermissionOptionKind::RejectAlways => {
                                "Always deny this kind of request".to_string()
                            }
                        }),
                    })
                    .collect(),
                selected_option: None,
                decision: None,
            }),
            AgentEvent::ConfigOptionsChanged(_) | AgentEvent::CommandsChanged(_) => None,
        })
        .collect()
}

pub fn normalize_event(event: AgentEvent, created_at: &str) -> NormalizedMessage {
    normalize_events(vec![event], created_at)
        .into_iter()
        .next()
        .expect("single event normalizes to one message")
}

fn tool_call_status(status: AgentToolCallStatus) -> ActivityStatus {
    match status {
        AgentToolCallStatus::Pending | AgentToolCallStatus::InProgress => ActivityStatus::Running,
        AgentToolCallStatus::Completed => ActivityStatus::Completed,
        AgentToolCallStatus::Failed => ActivityStatus::Error,
    }
}

#[cfg(test)]
mod tests;
