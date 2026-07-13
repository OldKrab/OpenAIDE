use serde_json::json;

use super::*;

#[test]
fn task_send_message_is_text_plus_ordered_attachment_handles() {
    let params = TaskSendParams {
        task_id: "task-1".into(),
        message: ComposerMessage {
            text: Some("/plan implement this".to_string()),
            attachments: vec!["handle-1".into(), "handle-2".into()],
        },
    };

    let value = serde_json::to_value(params).unwrap();

    assert_eq!(value["taskId"], json!("task-1"));
    assert!(value.get("taskRevision").is_none());
    assert!(value.get("idempotencyKey").is_none());
    assert_eq!(value["message"]["text"], json!("/plan implement this"));
    assert_eq!(
        value["message"]["attachments"],
        json!(["handle-1", "handle-2"])
    );
}

#[test]
fn task_list_filter_is_optional_for_global_history() {
    let value = serde_json::to_value(TaskListParams {
        archived: false,
        project_id: None,
        cursor: None,
    })
    .unwrap();

    assert!(value.get("projectId").is_none());
    assert!(value.get("cursor").is_none());
}
