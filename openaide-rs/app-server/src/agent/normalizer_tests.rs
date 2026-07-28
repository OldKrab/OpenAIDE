use crate::agent::events::{AgentEvent, AgentSubagent};
use crate::agent::normalizer::{normalize_event, normalize_events};
use crate::protocol::model::{
    ActivityStatus, ActivityStep, AgentCommandsCatalog, ConfigOptionsCatalog, NormalizedMessage,
};

#[test]
fn session_catalog_updates_do_not_create_chat_messages() {
    let messages = normalize_events(
        vec![
            AgentEvent::CommandsChanged(AgentCommandsCatalog::default()),
            AgentEvent::ConfigOptionsChanged(ConfigOptionsCatalog::empty("codex")),
        ],
        "2026-07-07T00:00:00.000Z",
    );

    assert!(messages.is_empty());
}

#[test]
fn subagent_tool_preserves_agent_title_and_protocol_details() {
    let message = normalize_event(
        AgentEvent::Subagent(AgentSubagent {
            tool_call_id: "call_start_correctness".to_string(),
            title: "Start subagent correctness".to_string(),
            thread_id: "thread_correctness".to_string(),
            path: "/root/review/correctness".to_string(),
            activity: "started".to_string(),
            status: ActivityStatus::Completed,
        }),
        "2026-07-27T00:00:00.000Z",
    );

    assert!(matches!(
        message,
        NormalizedMessage::Activity {
            id,
            title,
            status: ActivityStatus::Completed,
            steps,
            ..
        } if id == "acp_tool:call_start_correctness"
            && title == "Start subagent correctness"
            && matches!(
                steps.as_slice(),
                [ActivityStep::Subagent {
                    tool_call_id,
                    title,
                    thread_id,
                    path,
                    activity,
                    status: ActivityStatus::Completed,
                    ..
                }]
                    if tool_call_id.as_deref() == Some("call_start_correctness")
                        && title.as_deref() == Some("Start subagent correctness")
                        && thread_id.as_deref() == Some("thread_correctness")
                        && path == &["review", "correctness"]
                        && activity.as_deref() == Some("started")
            )
    ));
}
