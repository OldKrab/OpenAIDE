use serde_json::json;

use super::*;

#[test]
fn task_file_search_is_scoped_and_uses_relative_paths() {
    let params = TaskSearchFilesParams {
        task_id: TaskId::new("task-1"),
        query: "main".to_string(),
    };
    assert_eq!(
        serde_json::to_value(params).unwrap(),
        serde_json::json!({ "taskId": "task-1", "query": "main" })
    );

    let result = TaskSearchFilesResult {
        task_id: TaskId::new("task-1"),
        state: WorkspaceFileSearchState::Ready,
        paths: vec!["src/main.rs".to_string()],
        notice: None,
    };
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::json!({
            "taskId": "task-1",
            "state": "ready",
            "paths": ["src/main.rs"]
        })
    );
}

#[test]
fn task_send_message_is_text_plus_inline_ordered_images() {
    let params = TaskSendParams {
        task_id: "task-1".into(),
        message: ComposerMessage {
            text: Some("/plan implement this".to_string()),
            images: vec![ComposerImage {
                label: "diagram.png".to_string(),
                mime_type: "image/png".to_string(),
                data: "iVBORw0KGgo=".to_string(),
            }],
        },
    };

    let value = serde_json::to_value(params).unwrap();

    assert_eq!(value["taskId"], json!("task-1"));
    assert!(value.get("taskRevision").is_none());
    assert!(value.get("idempotencyKey").is_none());
    assert_eq!(value["message"]["text"], json!("/plan implement this"));
    assert_eq!(value["message"]["images"][0]["label"], json!("diagram.png"));
    assert_eq!(
        value["message"]["images"][0]["mimeType"],
        json!("image/png")
    );
    assert_eq!(value["message"]["images"][0]["data"], json!("iVBORw0KGgo="));
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
