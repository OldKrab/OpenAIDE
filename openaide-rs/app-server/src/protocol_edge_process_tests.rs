use openaide_app_server::task_events::{CommittedTaskDelta, TaskUpdate};
use openaide_app_server_protocol::events::TextChunk;
use openaide_app_server_protocol::ids::MessageId;

use super::forward_local_http_task_updates;

#[test]
fn local_http_handoff_forwards_committed_delta_without_reducing_it_to_task_id() {
    let (sender, updates) = std::sync::mpsc::channel();
    sender
        .send(TaskUpdate::committed(
            "task-1",
            2,
            CommittedTaskDelta::ChatItemChunk {
                message_id: MessageId::from("message-1"),
                chunk: TextChunk {
                    sequence: 1,
                    text: "raw chunk".to_string(),
                    final_chunk: false,
                },
            },
        ))
        .unwrap();
    drop(sender);
    let mut forwarded = Vec::new();

    forward_local_http_task_updates(updates, |update| forwarded.push(update));

    assert!(matches!(
        &forwarded[0].delta,
        Some(CommittedTaskDelta::ChatItemChunk { chunk, .. })
            if chunk.sequence == 1 && chunk.text == "raw chunk"
    ));
}
