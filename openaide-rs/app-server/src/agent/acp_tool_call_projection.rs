use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::{ToolCall, ToolCallUpdate};

pub(super) type ToolCallState = Arc<Mutex<HashMap<String, ToolCall>>>;

pub(super) fn remember_tool_call(tool_calls: &ToolCallState, tool_call: ToolCall) {
    tool_calls
        .lock()
        .expect("ACP tool call state lock poisoned")
        .insert(tool_call.tool_call_id.to_string(), tool_call);
}

pub(super) fn merge_tool_call_update(
    tool_calls: &ToolCallState,
    update: ToolCallUpdate,
) -> ToolCall {
    let tool_call_id = update.tool_call_id.to_string();
    let mut tool_calls = tool_calls
        .lock()
        .expect("ACP tool call state lock poisoned");
    let tool_call = if let Some(tool_call) = tool_calls.get_mut(&tool_call_id) {
        tool_call.update(update.fields);
        tool_call.clone()
    } else {
        tool_call_from_update(update)
    };
    tool_calls.insert(tool_call_id, tool_call.clone());
    tool_call
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
