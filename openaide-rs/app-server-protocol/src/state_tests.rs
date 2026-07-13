use serde_json::json;

use super::*;

#[test]
fn task_subscription_scope_uses_typed_kind_and_task_id() {
    let value = serde_json::to_value(StateSubscribeParams {
        scope: SubscriptionScope::Task {
            task_id: "task-1".into(),
        },
    })
    .unwrap();

    assert_eq!(value["scope"]["kind"], json!("task"));
    assert_eq!(value["scope"]["taskId"], json!("task-1"));
}

#[test]
fn task_navigation_scope_can_be_global_or_project_filtered() {
    let global =
        serde_json::to_value(SubscriptionScope::TaskNavigation { project_id: None }).unwrap();
    let project = serde_json::to_value(SubscriptionScope::TaskNavigation {
        project_id: Some("project-1".into()),
    })
    .unwrap();

    assert_eq!(global["kind"], json!("taskNavigation"));
    assert!(global.get("projectId").is_none());
    assert_eq!(project["projectId"], json!("project-1"));
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
