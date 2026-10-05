//! Agent-specific `_meta` that OpenAIDE attaches to `session/new`, `session/load`, and
//! `session/resume`.
//!
//! ACP reserves `_meta` for extensions, so every value here is a documented option of one
//! specific adapter. Agents without an entry receive no `_meta`.

use serde_json::json;

use crate::agent::acp_schema::Meta;
use crate::agent::registry::CLAUDE_CODE_AGENT_ID;

#[cfg(test)]
#[path = "acp_session_meta_tests.rs"]
mod tests;

/// Returns the `_meta` for session requests to `agent_id`.
///
/// The built-in Claude adapter forwards `_meta.claudeCode.options` to the Claude SDK. Recent
/// Claude models default `thinking.display` to `omitted`, which streams signature-only thinking
/// blocks with no text, so a long reasoning step is invisible in Chat. Requesting `summarized`
/// makes the adapter emit the summary as an `agent_thought_chunk`.
///
/// The value must be identical on every session request: the adapter fingerprints these options
/// and rebuilds its session when they differ between load and resume.
pub(super) fn session_request_meta(agent_id: &str) -> Option<Meta> {
    if agent_id != CLAUDE_CODE_AGENT_ID {
        return None;
    }
    let serde_json::Value::Object(meta) = json!({
        "claudeCode": {
            "options": {
                "thinking": { "type": "adaptive", "display": "summarized" },
            },
        },
    }) else {
        unreachable!("session meta literal is a JSON object");
    };
    Some(meta)
}
