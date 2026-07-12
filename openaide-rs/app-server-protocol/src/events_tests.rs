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
            "sequence": 2,
            "text": " world",
            "finalChunk": true
        }
    });

    let payload: AppServerEventPayload = serde_json::from_value(wire_payload).unwrap();
    let encoded = serde_json::to_value(payload).unwrap();

    assert_eq!(encoded["revision"], json!(7));
}
