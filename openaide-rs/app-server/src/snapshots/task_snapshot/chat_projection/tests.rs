use super::*;

#[test]
fn projects_thought_messages_as_system_chat_items() {
    let item = project_chat_item(&ChatMessage {
        cursor: "msg-1".to_string(),
        identity: "msg-1".to_string(),
        message_type: "thought".to_string(),
        message_id: "msg-1".to_string(),
        message: NormalizedMessage::Thought {
            id: "msg-1".to_string(),
            text: "Check current files first.".to_string(),
            created_at: "2026-06-29T10:00:00Z".to_string(),
            streaming: true,
        },
    });

    assert_eq!(item.role, ChatRole::System);
    assert_eq!(item.status, ChatItemStatus::Streaming);
    assert_eq!(
        item.parts,
        vec![MessagePart::Text {
            text: "Check current files first.".to_string(),
        }]
    );
}
