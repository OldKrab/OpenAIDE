use serde::Deserialize;
use serde_json::json;

use crate::agent::acp_schema::ToolCallUpdate;
use crate::agent::events::AgentTerminalAppend;
use crate::agent::registry::CODEX_AGENT_ID;
use crate::logging;

const TERMINAL_OUTPUT_DELTA_KEY: &str = "terminal_output_delta";
const MAX_TERMINAL_ID_BYTES: usize = 512;
const MAX_TERMINAL_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexTerminalOutputDelta {
    terminal_id: String,
    data: String,
}

/// Adapts the Codex ACP metadata extension into an ordered product delta.
///
/// ACP reserves `_meta` for implementation-defined data, so this adapter is
/// intentionally Agent-specific. Other Agents must negotiate and implement
/// their own append semantics instead of inheriting Codex assumptions.
pub(super) fn terminal_append(
    agent_id: &str,
    update: &ToolCallUpdate,
) -> Option<AgentTerminalAppend> {
    let value = update.meta.as_ref()?.get(TERMINAL_OUTPUT_DELTA_KEY)?;
    if agent_id != CODEX_AGENT_ID {
        ignored("unsupported_agent");
        return None;
    }
    let delta: CodexTerminalOutputDelta = match serde_json::from_value(value.clone()) {
        Ok(delta) => delta,
        Err(_) => {
            ignored("invalid_shape");
            return None;
        }
    };
    if delta.terminal_id.is_empty()
        || delta.terminal_id.len() > MAX_TERMINAL_ID_BYTES
        || delta.terminal_id.chars().any(char::is_control)
    {
        ignored("invalid_terminal_id");
        return None;
    }
    if delta.data.is_empty() || delta.data.len() > MAX_TERMINAL_CHUNK_BYTES {
        ignored("invalid_chunk_size");
        return None;
    }
    Some(AgentTerminalAppend {
        tool_call_id: update.tool_call_id.to_string(),
        terminal_id: delta.terminal_id,
        data: delta.data,
    })
}

fn ignored(reason_code: &'static str) {
    // Never log raw metadata, terminal identity, terminal bytes, or paths.
    logging::warn(
        "acp_terminal_output_delta_ignored",
        json!({ "reason_code": reason_code }),
    );
}

#[cfg(test)]
#[path = "acp_terminal_output_adapter_tests.rs"]
mod tests;
