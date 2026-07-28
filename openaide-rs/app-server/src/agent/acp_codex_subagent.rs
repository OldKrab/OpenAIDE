use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::acp_schema::{Meta, ToolCall, ToolCallStatus, ToolCallUpdate};
use crate::agent::events::{AgentSubagent, AgentToolCall, AgentToolCallStatus};
use crate::protocol::model::ActivityStatus;

/// Outcome of inspecting a Codex Tool event at the integration boundary.
pub(super) enum CodexSubagentProjection {
    Event(AgentSubagent),
    /// A follow-up for an already recognized lifecycle Tool; do not leak it as generic output.
    Suppress,
    GenericTool,
}

/// Correlates Codex's vendor-specific lifecycle events across prompts and replay.
#[derive(Clone, Default)]
pub(super) struct CodexSubagentState {
    inner: Arc<Mutex<CodexSubagentStateInner>>,
}

#[derive(Default)]
struct CodexSubagentStateInner {
    by_tool_call: HashMap<String, AgentSubagent>,
}

impl CodexSubagentState {
    pub(super) fn project_tool_call(&self, tool_call: &ToolCall) -> CodexSubagentProjection {
        let Some(fields) =
            parse_subagent_fields(tool_call.raw_input.as_ref(), tool_call.meta.as_ref())
        else {
            return CodexSubagentProjection::GenericTool;
        };
        self.record(AgentSubagent {
            tool_call_id: tool_call.tool_call_id.to_string(),
            title: tool_call.title.clone(),
            thread_id: fields.thread_id,
            path: fields.path,
            activity: fields.activity,
            status: activity_status(&tool_call.status),
        })
    }

    pub(super) fn project_tool_update(&self, update: &ToolCallUpdate) -> CodexSubagentProjection {
        let tool_call_id = update.tool_call_id.to_string();
        let existing = self
            .inner
            .lock()
            .expect("Codex subagent state lock poisoned")
            .by_tool_call
            .get(&tool_call_id)
            .cloned();
        let parsed = parse_subagent_fields(update.fields.raw_input.as_ref(), update.meta.as_ref());
        let Some(existing_or_fields) = existing
            .as_ref()
            .map(|existing| SubagentFields {
                thread_id: existing.thread_id.clone(),
                path: existing.path.clone(),
                activity: existing.activity.clone(),
            })
            .or(parsed.clone())
        else {
            return CodexSubagentProjection::GenericTool;
        };
        let fields = parsed.unwrap_or(existing_or_fields);
        let subagent = AgentSubagent {
            tool_call_id,
            title: update
                .fields
                .title
                .clone()
                .or_else(|| existing.as_ref().map(|value| value.title.clone()))
                .unwrap_or_else(|| "Subagent".to_string()),
            thread_id: fields.thread_id,
            path: fields.path,
            activity: fields.activity,
            status: update
                .fields
                .status
                .as_ref()
                .map(activity_status)
                .or_else(|| existing.as_ref().map(|value| value.status))
                .unwrap_or(ActivityStatus::Completed),
        };
        self.record(subagent)
    }

    fn record(&self, subagent: AgentSubagent) -> CodexSubagentProjection {
        let mut inner = self
            .inner
            .lock()
            .expect("Codex subagent state lock poisoned");
        if inner
            .by_tool_call
            .get(&subagent.tool_call_id)
            .is_some_and(|existing| existing == &subagent)
        {
            return CodexSubagentProjection::Suppress;
        }
        inner
            .by_tool_call
            .insert(subagent.tool_call_id.clone(), subagent.clone());
        CodexSubagentProjection::Event(subagent)
    }
}

#[derive(Clone)]
struct SubagentFields {
    thread_id: String,
    path: String,
    activity: String,
}

/// Prefer Codex metadata, with raw input as the compatibility fallback observed in traces.
fn parse_subagent_fields(
    raw_input: Option<&serde_json::Value>,
    meta: Option<&Meta>,
) -> Option<SubagentFields> {
    let metadata = meta
        .and_then(|meta| meta.get("codex"))
        .and_then(|codex| codex.get("subagent"));
    let thread_id =
        string_field(metadata, "threadId").or_else(|| string_field(raw_input, "agentThreadId"))?;
    let raw_path =
        string_field(metadata, "path").or_else(|| string_field(raw_input, "agentPath"))?;
    let activity =
        string_field(metadata, "activity").or_else(|| string_field(raw_input, "activityKind"))?;
    if raw_path.is_empty() {
        return None;
    }
    Some(SubagentFields {
        thread_id: thread_id.to_string(),
        path: raw_path.to_string(),
        activity: activity.to_string(),
    })
}

fn string_field<'a>(value: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    value?.get(key)?.as_str().filter(|value| !value.is_empty())
}

/// Sanitizes recognized Codex collaboration tools before generic Tool detail projection.
pub(super) fn project_codex_collaboration(tool_call: &ToolCall) -> Option<AgentToolCall> {
    let collaboration = tool_call
        .meta
        .as_ref()?
        .get("codex")?
        .get("collaboration")?;
    let tool = collaboration.get("tool")?.as_str()?;
    let (title, input_summary) = match tool {
        "wait" => ("Wait for subagents", None),
        _ => return None,
    };
    Some(AgentToolCall {
        tool_call_id: tool_call.tool_call_id.to_string(),
        scope_id: None,
        title: title.to_string(),
        kind: "collaboration".to_string(),
        status: match tool_call.status {
            ToolCallStatus::Pending => AgentToolCallStatus::Pending,
            ToolCallStatus::InProgress => AgentToolCallStatus::InProgress,
            ToolCallStatus::Completed => AgentToolCallStatus::Completed,
            ToolCallStatus::Failed => AgentToolCallStatus::Failed,
            _ => AgentToolCallStatus::Pending,
        },
        presentation: None,
        input_summary,
        output_preview: None,
        details: None,
    })
}

fn activity_status(status: &ToolCallStatus) -> ActivityStatus {
    match status {
        ToolCallStatus::Pending | ToolCallStatus::InProgress => ActivityStatus::Running,
        ToolCallStatus::Completed => ActivityStatus::Completed,
        ToolCallStatus::Failed => ActivityStatus::Error,
        _ => ActivityStatus::Running,
    }
}
