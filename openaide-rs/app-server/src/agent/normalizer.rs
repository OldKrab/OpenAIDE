use uuid::Uuid;

use crate::agent::events::{AgentEvent, AgentToolCallStatus};
use crate::protocol::model::{ActivityStatus, ActivityStep, NormalizedMessage};

pub fn normalize_events(events: Vec<AgentEvent>, created_at: &str) -> Vec<NormalizedMessage> {
    events
        .into_iter()
        .filter_map(|event| match event {
            AgentEvent::MessageChunk { role, part, .. } => Some(NormalizedMessage::AgentMessage {
                id: Uuid::new_v4().to_string(),
                role,
                parts: vec![part],
                created_at: created_at.to_string(),
            }),
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
                        permission_outcomes: Vec::new(),
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
                    permission_outcomes: Vec::new(),
                }],
            }),
            AgentEvent::PermissionRequest(_) => None,
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
#[path = "normalizer_tests.rs"]
mod tests;
