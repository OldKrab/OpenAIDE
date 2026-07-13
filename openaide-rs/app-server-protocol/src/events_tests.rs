use serde_json::json;

use super::AppServerEventPayload;

#[test]
fn chat_item_chunk_preserves_task_revision_on_the_wire() {
    let wire_payload = json!({
        "kind": "chatItemChunk",
        "taskId": "task-1",
        "revision": 7,
        "messageId": "message-1",
        "chunk": {
            "text": " world"
        }
    });

    let payload: AppServerEventPayload = serde_json::from_value(wire_payload).unwrap();
    let encoded = serde_json::to_value(payload).unwrap();

    assert_eq!(encoded["revision"], json!(7));
    assert_eq!(encoded["chunk"], json!({ "text": " world" }));
}

#[test]
fn tool_summary_and_full_detail_are_distinct_event_shapes() {
    let summary: AppServerEventPayload = serde_json::from_value(json!({
        "kind": "chatItemUpserted",
        "taskId": "task-1",
        "revision": 8,
        "item": {
            "messageId": "tool-1",
            "role": "system",
            "status": "complete",
            "parts": []
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
