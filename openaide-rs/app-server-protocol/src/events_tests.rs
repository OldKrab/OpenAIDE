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
        "kind": "toolDetailChanged",
        "taskId": "task-1",
        "artifactId": "artifact-1",
        "revision": 3,
        "deltas": [{ "kind": "appendTerminal", "terminalId": "terminal-1", "data": "complete output" }]
    }))
    .unwrap();

    assert_eq!(serde_json::to_value(summary).unwrap()["revision"], json!(8));
    assert_eq!(
        serde_json::to_value(detail).unwrap()["deltas"][0]["data"],
        json!("complete output")
    );
}

#[test]
fn navigation_frame_has_a_distinct_wire_event() {
    let payload: AppServerEventPayload = serde_json::from_value(json!({
        "kind": "navigationFrame",
        "frame": {
            "section": "tasks",
            "groupOrder": ["project-api"],
            "groups": [{
                "projectId": "project-api",
                "projectLabel": "api",
                "order": ["task:task-1"],
                "rows": {},
                "total": 1,
                "hidden": 0,
                "undiscovered": false,
                "discovery": "settled"
            }]
        }
    }))
    .unwrap();

    let encoded = serde_json::to_value(payload).unwrap();
    assert_eq!(encoded["kind"], json!("navigationFrame"));
    assert_eq!(
        encoded["frame"]["groups"][0]["order"][0],
        json!("task:task-1")
    );
}
