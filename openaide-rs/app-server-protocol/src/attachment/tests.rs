use serde_json::json;

use super::{
    AttachmentConfirmEmbeddedParams, AttachmentCreateEmbeddedCandidateParams,
    AttachmentCreateFileReferenceParams, AttachmentCreatePastedImageParams,
    AttachmentListDirectoryParams, AttachmentListRootsParams, AttachmentRevealParams,
};

#[test]
fn attachment_file_browser_params_are_task_scoped_and_opaque() {
    let roots = serde_json::to_value(AttachmentListRootsParams {
        task_id: "task-1".into(),
    })
    .unwrap();
    assert_eq!(roots, json!({ "taskId": "task-1" }));

    let list = serde_json::to_value(AttachmentListDirectoryParams {
        task_id: "task-1".into(),
        root_id: "root-1".into(),
        directory_id: Some("entry-1".into()),
    })
    .unwrap();
    assert_eq!(
        list,
        json!({ "taskId": "task-1", "rootId": "root-1", "directoryId": "entry-1" })
    );

    let create = serde_json::to_value(AttachmentCreateFileReferenceParams {
        task_id: "task-1".into(),
        entry_id: "entry-2".into(),
    })
    .unwrap();
    assert_eq!(create, json!({ "taskId": "task-1", "entryId": "entry-2" }));

    let pasted_image = serde_json::to_value(AttachmentCreatePastedImageParams {
        task_id: "task-1".into(),
        label: "Screenshot.png".to_string(),
        mime_type: "image/png".to_string(),
        data: "aW1hZ2U=".to_string(),
    })
    .unwrap();
    assert_eq!(
        pasted_image,
        json!({
            "taskId": "task-1",
            "label": "Screenshot.png",
            "mimeType": "image/png",
            "data": "aW1hZ2U="
        })
    );

    let candidate = serde_json::to_value(AttachmentCreateEmbeddedCandidateParams {
        task_id: "task-1".into(),
        entry_id: "entry-3".into(),
    })
    .unwrap();
    assert_eq!(
        candidate,
        json!({ "taskId": "task-1", "entryId": "entry-3" })
    );

    let confirm = serde_json::to_value(AttachmentConfirmEmbeddedParams {
        task_id: "task-1".into(),
        candidates: vec!["candidate-1".into()],
    })
    .unwrap();
    assert_eq!(
        confirm,
        json!({ "taskId": "task-1", "candidates": ["candidate-1"] })
    );

    let reveal = serde_json::to_value(AttachmentRevealParams {
        task_id: "task-1".into(),
        handle_id: "attachment-handle-1".into(),
    })
    .unwrap();
    assert_eq!(
        reveal,
        json!({ "taskId": "task-1", "handleId": "attachment-handle-1" })
    );
}
