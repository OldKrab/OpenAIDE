use serde_json::json;

use super::session_request_meta;

#[test]
fn claude_code_requests_summarized_thinking() {
    let meta = session_request_meta("claude-code").expect("Claude Code has session meta");

    assert_eq!(
        serde_json::Value::Object(meta),
        json!({
            "claudeCode": {
                "options": { "thinking": { "type": "adaptive", "display": "summarized" } },
            },
        })
    );
}

#[test]
fn other_agents_receive_no_session_meta() {
    for agent_id in ["codex", "opencode", "custom.1234"] {
        assert!(session_request_meta(agent_id).is_none(), "{agent_id}");
    }
}
