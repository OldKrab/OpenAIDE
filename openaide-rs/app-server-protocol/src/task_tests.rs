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
        queue_selection: None,
        message: ComposerMessage {
            text: Some("/plan implement this".to_string()),
            images: vec![ComposerImage {
                label: "diagram.png".to_string(),
                mime_type: "image/png".to_string(),
                data: "iVBORw0KGgo=".to_string(),
            }],
            attachments: Vec::new(),
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
        lifecycle: TaskListLifecycle::Open,
        project_id: None,
        cursor: None,
    })
    .unwrap();

    assert_eq!(value["lifecycle"], json!("open"));
    assert!(value.get("projectId").is_none());
    assert!(value.get("cursor").is_none());
    assert!(value.get("archived").is_none());
}

#[test]
fn task_set_title_selects_user_or_automatic_ownership() {
    assert_eq!(
        serde_json::to_value(TaskSetTitleParams {
            task_id: "task-1".into(),
            title: TaskTitleSelection::User {
                value: "My title".to_string(),
            },
        })
        .unwrap(),
        json!({
            "taskId": "task-1",
            "title": { "kind": "user", "value": "My title" }
        })
    );
    assert_eq!(
        serde_json::to_value(TaskTitleSelection::Automatic).unwrap(),
        json!({ "kind": "automatic" })
    );
}

#[test]
fn task_set_pinned_carries_authoritative_boolean_intent() {
    assert_eq!(
        serde_json::to_value(TaskSetPinnedParams {
            task_id: "task-1".into(),
            pinned: true,
        })
        .unwrap(),
        json!({
            "taskId": "task-1",
            "pinned": true
        })
    );
}

#[test]
fn task_attention_wait_first_request_omits_the_cursor() {
    assert_eq!(
        serde_json::to_value(TaskAttentionWaitParams::default()).unwrap(),
        json!({})
    );
    assert_eq!(
        serde_json::to_value(TaskAttentionWaitParams {
            cursor: Some("3f2a".to_string()),
        })
        .unwrap(),
        json!({ "cursor": "3f2a" })
    );
    let result: TaskAttentionWaitResult =
        serde_json::from_value(json!({ "cursor": "3f2a", "tasks": [] })).unwrap();
    assert_eq!(result.cursor, "3f2a");
    assert!(result.tasks.is_empty());
}

#[test]
fn tool_image_preview_request_uses_only_task_and_artifact_identity() {
    assert_eq!(
        serde_json::to_value(TaskToolImagePreviewParams {
            task_id: "task-1".into(),
            artifact_id: "artifact-1".to_string(),
        })
        .unwrap(),
        json!({
            "taskId": "task-1",
            "artifactId": "artifact-1"
        })
    );
}

#[test]
fn task_image_request_without_a_variant_still_means_the_original() {
    // An older client sends no variant and must keep getting the stored bytes.
    let older: TaskImageParams =
        serde_json::from_value(json!({ "taskId": "task-1", "imageId": "abc" })).unwrap();
    assert_eq!(older.variant, None);

    let preview = TaskImageParams {
        task_id: TaskId::new("task-1"),
        image_id: "abc".to_string(),
        variant: Some(TaskImageVariant::Preview),
    };
    assert_eq!(
        serde_json::to_value(preview).unwrap(),
        json!({ "taskId": "task-1", "imageId": "abc", "variant": "preview" })
    );

    // A result that is not a re-encoded preview carries no media type, so an older
    // client sees exactly the shape it knows.
    let original = TaskImageResult {
        data: "aGk=".to_string(),
        media_type: None,
    };
    assert_eq!(
        serde_json::to_value(original).unwrap(),
        json!({ "data": "aGk=" })
    );
}

#[test]
fn navigation_search_hit_carries_section_snippet_and_message() {
    let params: TaskNavigationSearchParams =
        serde_json::from_value(json!({ "text": "parser" })).unwrap();
    assert_eq!(params.project_ids, None);

    let hit = TaskNavigationSearchHit {
        row: crate::snapshot::TaskNavigationEntry::NativeSession {
            session: crate::snapshot::NativeSessionSummary {
                reference: crate::snapshot::NativeSessionReference {
                    agent_id: AgentId::new("agent-a"),
                    session_id: "native-1".to_string(),
                },
                project_id: ProjectId::new("project-1"),
                workspace_root: "/workspace".to_string(),
                worktree_id: None,
                title: Some("Parser".to_string()),
                pinned: false,
                last_activity: None,
            },
        },
        section: TaskNavigationSection::Archive,
        title_highlights: vec![(0, 6)],
        snippet: Some(SearchSnippet {
            text: "the parser".to_string(),
            highlights: vec![(4, 10)],
        }),
        message_id: Some(MessageId::new("message-1")),
    };
    let value = serde_json::to_value(hit).unwrap();

    assert_eq!(value["section"], "archive");
    assert_eq!(value["titleHighlights"], json!([[0, 6]]));
    assert_eq!(value["snippet"]["highlights"], json!([[4, 10]]));
    assert_eq!(value["messageId"], "message-1");
    assert_eq!(value["row"]["kind"], "nativeSession");
}
