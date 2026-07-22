use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::acp_schema::{ToolCall, ToolCallStatus, ToolCallUpdate};

#[cfg(test)]
#[path = "acp_tool_call_projection_tests.rs"]
mod tests;

pub(super) type ToolCallState = Arc<Mutex<HashMap<String, ToolCall>>>;

pub(super) fn remember_tool_call(tool_calls: &ToolCallState, tool_call: ToolCall) {
    let _ = remember_tool_call_with_status_change(tool_calls, tool_call);
}

pub(super) fn remember_tool_call_with_status_change(
    tool_calls: &ToolCallState,
    tool_call: ToolCall,
) -> bool {
    let tool_call_id = tool_call.tool_call_id.to_string();
    let mut tool_calls = tool_calls
        .lock()
        .expect("ACP tool call state lock poisoned");
    let status_changed = tool_calls
        .get(&tool_call_id)
        .is_none_or(|existing| existing.status != tool_call.status);
    tool_calls.insert(tool_call_id, tool_call);
    status_changed
}

pub(super) fn merge_tool_call_update(
    tool_calls: &ToolCallState,
    update: ToolCallUpdate,
) -> ToolCall {
    merge_tool_call_update_with_status_change(tool_calls, update).0
}

pub(super) fn merge_tool_call_update_with_status_change(
    tool_calls: &ToolCallState,
    update: ToolCallUpdate,
) -> (ToolCall, bool) {
    let (tool_call, status_changed, _) = merge_tool_call_update_with_changes(tool_calls, update);
    (tool_call, status_changed)
}

/// Reports whether the normalized Tool projection actually changed. ACP `_meta`
/// extensions are intentionally excluded because they need named adapters and
/// must not trigger unchanged Task persistence by accident.
pub(super) fn merge_tool_call_update_with_changes(
    tool_calls: &ToolCallState,
    update: ToolCallUpdate,
) -> (ToolCall, bool, bool) {
    let tool_call_id = update.tool_call_id.to_string();
    let mut tool_calls = tool_calls
        .lock()
        .expect("ACP tool call state lock poisoned");
    let previous = tool_calls.get(&tool_call_id).cloned();
    let previous_status = previous.as_ref().map(|tool_call| tool_call.status);
    let tool_call = if let Some(tool_call) = tool_calls.get_mut(&tool_call_id) {
        tool_call.update(update.fields);
        tool_call.clone()
    } else {
        tool_call_from_update(update)
    };
    tool_calls.insert(tool_call_id, tool_call.clone());
    let status_changed = previous_status.is_none_or(|status| status != tool_call.status);
    let projection_changed = previous.as_ref() != Some(&tool_call);
    (tool_call, status_changed, projection_changed)
}

fn tool_call_from_update(update: ToolCallUpdate) -> ToolCall {
    let title = update
        .fields
        .title
        .clone()
        .unwrap_or_else(|| "Tool call".to_string());
    let mut tool_call = ToolCall::new(update.tool_call_id, title);
    if let Some(kind) = update.fields.kind {
        tool_call = tool_call.kind(kind);
    }
    if let Some(status) = update.fields.status {
        tool_call = tool_call.status(status);
    }
    if let Some(content) = update.fields.content {
        tool_call = tool_call.content(content);
    }
    if let Some(locations) = update.fields.locations {
        tool_call = tool_call.locations(locations);
    }
    if let Some(raw_input) = update.fields.raw_input {
        tool_call = tool_call.raw_input(raw_input);
    }
    if let Some(raw_output) = update.fields.raw_output {
        tool_call = tool_call.raw_output(raw_output);
    }
    tool_call
}

pub(super) fn tool_status_name(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::InProgress => "in_progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        _ => "other",
    }
}
