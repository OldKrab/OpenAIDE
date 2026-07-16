use super::*;

#[test]
fn projects_legacy_user_data_image_markdown_as_typed_message_parts() {
    let item = project_chat_item(&ChatMessage {
        cursor: "user-image".to_string(),
        identity: "user-image".to_string(),
        message_type: "user".to_string(),
        message_id: "user-image".to_string(),
        message: NormalizedMessage::User {
            id: "user-image".to_string(),
            text: "Before [@image](data:image/png;base64,aW1hZ2U=) after".to_string(),
            created_at: "2026-07-16T00:00:00Z".to_string(),
            attachments: vec![],
        },
    });

    assert_eq!(
        item.parts,
        vec![
            MessagePart::Text {
                text: "Before ".to_string(),
            },
            MessagePart::Image {
                media_type: "image/png".to_string(),
                data_url: "data:image/png;base64,aW1hZ2U=".to_string(),
                uri: None,
            },
            MessagePart::Text {
                text: " after".to_string(),
            },
        ]
    );
}

#[test]
fn projects_thought_messages_as_system_chat_items() {
    let item = project_chat_item(&ChatMessage {
        cursor: "msg-1".to_string(),
        identity: "msg-1".to_string(),
        message_type: "thought_message".to_string(),
        message_id: "msg-1".to_string(),
        message: NormalizedMessage::AgentMessage {
            id: "msg-1".to_string(),
            role: AgentMessageRole::Thought,
            parts: vec![AgentMessagePart::Text {
                text: "Check current files first.".to_string(),
            }],
            created_at: "2026-06-29T10:00:00Z".to_string(),
        },
    });

    assert_eq!(item.role, ChatRole::System);
    assert_eq!(item.status, ChatItemStatus::Complete);
    assert_eq!(
        item.parts,
        vec![MessagePart::Text {
            text: "Check current files first.".to_string(),
        }]
    );
}

#[test]
fn projects_one_agent_message_as_one_ordered_chat_item() {
    let item = project_chat_item(&ChatMessage {
        cursor: "msg-2".to_string(),
        identity: "msg-2".to_string(),
        message_type: "agent_message".to_string(),
        message_id: "msg-2".to_string(),
        message: NormalizedMessage::AgentMessage {
            id: "msg-2".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![
                AgentMessagePart::Text {
                    text: "Before".to_string(),
                },
                AgentMessagePart::Resource {
                    uri: "file:///result.txt".to_string(),
                    name: Some("result.txt".to_string()),
                    title: None,
                    description: None,
                    media_type: Some("text/plain".to_string()),
                    size_bytes: None,
                    text: Some("Result".to_string()),
                },
                AgentMessagePart::Text {
                    text: "After".to_string(),
                },
            ],
            created_at: "2026-06-29T10:00:00Z".to_string(),
        },
    });

    assert_eq!(item.role, ChatRole::Agent);
    assert!(matches!(
        item.parts.as_slice(),
        [
            MessagePart::Text { text: before },
            MessagePart::Resource { uri, .. },
            MessagePart::Text { text: after },
        ] if before == "Before" && uri == "file:///result.txt" && after == "After"
    ));
}
