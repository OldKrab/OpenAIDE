use openaide_app_server::task_events::{CommittedTaskChange, TaskUpdate, TaskUpdateKind};
use openaide_app_server_protocol::events::{TaskChanges, TaskChatChange};
use openaide_app_server_protocol::ids::MessageId;

use super::forward_local_http_task_updates;

#[test]
fn local_http_handoff_forwards_committed_delta_without_reducing_it_to_task_id() {
    let (sender, updates) = std::sync::mpsc::channel();
    sender
        .send(TaskUpdate {
            task_id: "task-1".to_string(),
            revision: 2,
            kind: TaskUpdateKind::Changed(Box::new(CommittedTaskChange {
                lifecycle: None,
                changes: TaskChanges {
                    chat: vec![TaskChatChange::AppendText {
                        message_id: MessageId::from("message-1"),
                        text: "raw chunk".to_string(),
                    }],
                    ..TaskChanges::default()
                },
                tool_details: Vec::new(),
                navigation: None,
            })),
        })
        .unwrap();
    drop(sender);
    let mut forwarded = Vec::new();

    forward_local_http_task_updates(updates, |update| forwarded.push(update));

    assert!(matches!(
        &forwarded[0].kind,
        TaskUpdateKind::Changed(change)
            if matches!(change.changes.chat.as_slice(),
                [TaskChatChange::AppendText { text, .. }] if text == "raw chunk")
    ));
}
