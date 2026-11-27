use serde_json::json;

use super::*;

#[test]
fn task_subscription_scope_uses_typed_kind_and_task_id() {
    let value = serde_json::to_value(StateSubscribeParams {
        scope: SubscriptionScope::Task {
            task_id: "task-1".into(),
        },
        known_version: None,
    })
    .unwrap();

    assert_eq!(value["scope"]["kind"], json!("task"));
    assert_eq!(value["scope"]["taskId"], json!("task-1"));
}

#[test]
fn task_navigation_scope_identifies_only_the_section() {
    let value = serde_json::to_value(SubscriptionScope::TaskNavigation {
        section: crate::task::TaskNavigationSection::Archive,
    })
    .unwrap();

    assert_eq!(
        value,
        json!({"kind": "taskNavigation", "section": "archive"})
    );
}

#[test]
fn navigation_view_params_carry_window_and_open_row() {
    use crate::navigation::{NavigationRowRef, NavigationViewParams};

    let params: NavigationViewParams = serde_json::from_value(json!({
        "section": "tasks",
        "projectIds": ["project-api"],
        "historyLimit": 7,
        "historyLimits": [{"projectId": "project-api", "historyLimit": 17}],
        "openRow": {"kind": "nativeSession", "agentId": "agent-1", "sessionId": "s1"}
    }))
    .unwrap();

    assert_eq!(params.history_limit, 7);
    assert_eq!(params.history_limits[0].history_limit, 17);
    assert_eq!(
        params.open_row,
        Some(NavigationRowRef::NativeSession {
            agent_id: "agent-1".into(),
            session_id: "s1".to_string()
        })
    );
}

#[test]
fn tool_detail_scope_identifies_one_task_artifact() {
    let value = serde_json::to_value(SubscriptionScope::ToolDetail {
        task_id: "task-1".into(),
        artifact_id: "artifact-1".to_string(),
    })
    .unwrap();

    assert_eq!(
        value,
        json!({
            "kind": "toolDetail",
            "taskId": "task-1",
            "artifactId": "artifact-1",
        })
    );
}

#[test]
fn entity_reply_syncs_have_a_distinct_wire_form() {
    use crate::state::{SubagentChatSync, TerminalOutputAppend, ToolTerminalSync};

    assert_eq!(
        serde_json::to_value(ToolTerminalSync::Deltas {
            appends: vec![TerminalOutputAppend {
                terminal_id: "terminal-1".to_string(),
                data: "more".to_string(),
            }],
        })
        .unwrap(),
        json!({
            "kind": "deltas",
            "appends": [{ "terminalId": "terminal-1", "data": "more" }],
        })
    );
    assert_eq!(
        serde_json::to_value(ToolTerminalSync::Current).unwrap(),
        json!({ "kind": "current" })
    );
    assert_eq!(
        serde_json::to_value(SubagentChatSync::Full).unwrap(),
        json!({ "kind": "full" })
    );
}
