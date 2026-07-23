use serde_json::json;

use super::AppServerEventPayload;

#[test]
fn task_chat_append_text_preserves_task_revision_on_the_wire() {
    let wire_payload = json!({
        "kind": "taskChanged",
        "taskId": "task-1",
        "revision": 7,
        "changes": {
            "chat": [{
                "kind": "appendText",
                "messageId": "message-1",
                "text": " world"
            }]
        }
    });

    let payload: AppServerEventPayload = serde_json::from_value(wire_payload).unwrap();
    let encoded = serde_json::to_value(payload).unwrap();

    assert_eq!(encoded["revision"], json!(7));
    assert_eq!(encoded["changes"]["chat"][0]["text"], json!(" world"));
}

#[test]
fn tool_summary_and_full_detail_are_distinct_event_shapes() {
    let summary: AppServerEventPayload = serde_json::from_value(json!({
        "kind": "taskChanged",
        "taskId": "task-1",
        "revision": 8,
        "changes": {
            "chat": [{
                "kind": "upsert",
                "item": {
                    "messageId": "tool-1",
                    "role": "system",
                    "status": "complete",
                    "parts": []
                }
            }]
        }
    }))
    .unwrap();
    let detail: AppServerEventPayload = serde_json::from_value(json!({
        "kind": "toolDetailUpdated",
        "taskId": "task-1",
        "artifactId": "artifact-1",
        "details": {
            "locations": [],
            "content": [{ "kind": "text", "text": "complete output" }]
        }
    }))
    .unwrap();

    assert_eq!(serde_json::to_value(summary).unwrap()["revision"], json!(8));
    assert_eq!(
        serde_json::to_value(detail).unwrap()["details"]["content"][0]["text"],
        json!("complete output")
    );
}

#[test]
fn task_navigation_task_update_has_a_distinct_wire_event() {
    let payload: AppServerEventPayload = serde_json::from_value(json!({
        "kind": "taskUpdated",
        "projectId": "project-api",
        "task": {
            "taskId": "task-1",
            "projectId": "project-api",
            "agentId": "codex",
            "lifecycle": "open",
            "title": null,
            "status": "waiting",
            "updatedAt": "2026-07-23T12:00:00Z",
            "lastActivity": "2026-07-23T11:00:00Z",
            "unread": true,
            "attention": {
                "eventId": "permission-1",
                "reason": "needsPermission",
                "occurredAt": "2026-07-23T12:00:00Z"
            },
            "hasMessages": true,
            "workspaceAvailable": true
        }
    }))
    .unwrap();

    let encoded = serde_json::to_value(payload).unwrap();
    assert_eq!(encoded["kind"], json!("taskUpdated"));
    assert_eq!(encoded["task"]["status"], json!("waiting"));
}
