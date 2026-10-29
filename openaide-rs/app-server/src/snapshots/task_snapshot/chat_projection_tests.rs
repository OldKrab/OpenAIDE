use super::*;

#[test]
fn projects_stored_and_pending_image_references_and_omits_an_image_without_bytes() {
    let image_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    // SHA-256 of `image`: the id the journal writer stores a draft's bytes under.
    let pending_id = "6105d6cc76af400325e94d588ce511be5bfdbb73b437dc51eca43917d7a43e3d".to_string();
    let user = project_chat_item(&ChatMessage {
        cursor: "user-image".to_string(),
        identity: "user-image".to_string(),
        message_type: "user".to_string(),
        message_id: "user-image".to_string(),
        message: NormalizedMessage::User {
            id: "user-image".to_string(),
            text: "Look".to_string(),
            created_at: "2026-07-16T00:00:00Z".to_string(),
            attachments: vec![
                Attachment {
                    kind: "image".to_string(),
                    label: "Image".to_string(),
                    path: None,
                    payload: Some(serde_json::json!({
                        "imageId": image_id,
                        "mimeType": "image/png",
                        "sizeBytes": 5,
                        "width": 4,
                        "height": 3,
                    })),
                },
                Attachment {
                    kind: "image".to_string(),
                    label: "Draft".to_string(),
                    path: None,
                    payload: Some(serde_json::json!({
                        "data": "aW1hZ2U=",
                        "mimeType": "image/png",
                    })),
                },
            ],
        },
        timing: Default::default(),
    });
    assert_eq!(
        user.parts,
        vec![
            MessagePart::Text {
                text: "Look".to_string(),
            },
            MessagePart::Image {
                media_type: "image/png".to_string(),
                image_id: image_id.clone(),
                size_bytes: 5,
                dimensions: Some(openaide_app_server_protocol::snapshot::ImageDimensions {
                    width: 4,
                    height: 3,
                }),
                uri: None,
            },
            MessagePart::Image {
                media_type: "image/png".to_string(),
                image_id: pending_id.clone(),
                size_bytes: 5,
                dimensions: None,
                uri: None,
            },
        ]
    );

    let agent = project_chat_item(&ChatMessage {
        cursor: "agent-image".to_string(),
        identity: "agent-image".to_string(),
        message_type: "agent_message".to_string(),
        message_id: "agent-image".to_string(),
        message: NormalizedMessage::AgentMessage {
            id: "agent-image".to_string(),
            role: AgentMessageRole::Agent,
            parts: vec![
                AgentMessagePart::Image {
                    media_type: "image/png".to_string(),
                    data: String::new(),
                    image_id: Some(image_id.clone()),
                    size_bytes: Some(5),
                    width: None,
                    height: None,
                    uri: None,
                },
                AgentMessagePart::Image {
                    media_type: "image/png".to_string(),
                    data: "aW1hZ2U=".to_string(),
                    image_id: None,
                    size_bytes: None,
                    width: None,
                    height: None,
                    uri: None,
                },
                AgentMessagePart::Image {
                    media_type: "image/png".to_string(),
                    data: String::new(),
                    image_id: None,
                    size_bytes: None,
                    width: None,
                    height: None,
                    uri: None,
                },
            ],
            created_at: "2026-07-16T00:00:00Z".to_string(),
        },
        timing: Default::default(),
    });
    assert_eq!(
        agent.parts,
        vec![
            MessagePart::Image {
                media_type: "image/png".to_string(),
                image_id,
                size_bytes: 5,
                dimensions: None,
                uri: None,
            },
            MessagePart::Image {
                media_type: "image/png".to_string(),
                image_id: pending_id,
                size_bytes: 5,
                dimensions: None,
                uri: None,
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
        timing: Default::default(),
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
fn projects_execute_presentation_without_reclassifying_the_tool() {
    let step = project_activity_step(&ActivityStep::Tool {
        background_outcome: None,
        tool_call_id: Some("call-1".to_string()),
        name: "execute".to_string(),
        status: ActivityStatus::Completed,
        presentation: Some(crate::protocol::model::ToolPresentation::single(
            crate::protocol::model::ToolPresentationKind::Skill,
            vec!["tdd".to_string(), "impeccable".to_string()],
        )),
        description: None,
        input_summary: Some("sed -n ...".to_string()),
        output_preview: Some("skill contents".to_string()),
        detail_artifact_id: None,
        details: None,
        permission_outcomes: vec![],
        pending: false,
    });

    let ActivityStepSnapshot::Tool {
        name, presentation, ..
    } = step
    else {
        panic!("expected tool step");
    };
    assert_eq!(name, "execute");
    let presentation = presentation.expect("presentation");
    let [action] = presentation.actions.as_slice() else {
        panic!("expected one projected action");
    };
    assert_eq!(
        action,
        &openaide_app_server_protocol::snapshot::ToolPresentationActionSnapshot::Skill {
            subjects: vec!["tdd".to_string(), "impeccable".to_string()],
        }
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
        timing: Default::default(),
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
