use serde_json::json;

use super::*;
use crate::client::SettingsSection;

#[test]
fn client_snapshot_omits_categories_outside_scope() {
    let snapshot = ClientSnapshot {
        cursor: "cursor-1".into(),
        server: ServerSnapshot {
            server_id: "server-1".into(),
            protocol_version: ProtocolVersion::V1,
            capabilities: ServerCapabilities {
                reconnect: true,
                resync: true,
                streaming_events: true,
                frontend_requests: false,
            },
        },
        state_root: StateRootSnapshot {
            state_root_id: "state-root-1".into(),
        },
        client: ClientSnapshotScope {
            client_instance_id: "client-1".into(),
            shell_kind: ShellKind::Web,
            surface: RequestedSurface::Home,
        },
        projects: None,
        agents: None,
        tasks: None,
        active_task: None,
        settings: None,
        pending_requests: Vec::new(),
    };

    let value = serde_json::to_value(snapshot).unwrap();

    assert_eq!(value["cursor"], json!("cursor-1"));
    assert!(value.get("projects").is_none());
    assert!(value.get("activeTask").is_none());
    assert_eq!(value["server"]["protocolVersion"]["major"], json!(1));
    assert_eq!(
        value["server"]["capabilities"]["streamingEvents"],
        json!(true)
    );
}

#[test]
fn chat_is_part_based() {
    let item = ChatItem {
        message_id: "message-1".into(),
        turn_id: Some("turn-1".into()),
        role: ChatRole::User,
        status: ChatItemStatus::Complete,
        parts: vec![
            MessagePart::Text {
                text: "check this".to_string(),
            },
            MessagePart::Attachment {
                attachment: AttachmentSnapshot {
                    attachment_id: "attachment-1".into(),
                    kind: AttachmentKind::FileReference,
                    label: "src/main.rs".to_string(),
                    media_type: Some("text/x-rust".to_string()),
                    size_bytes: Some(42),
                    preview_url: None,
                },
            },
        ],
    };

    let value = serde_json::to_value(item).unwrap();

    assert_eq!(value["parts"][0]["kind"], json!("text"));
    assert_eq!(value["parts"][1]["kind"], json!("attachment"));
    assert!(value["parts"][1]["attachment"].get("path").is_none());
}

#[test]
fn public_enums_match_product_language() {
    assert_eq!(
        serde_json::to_value(AgentStatus::Connected).unwrap(),
        json!("connected")
    );
    assert_eq!(
        serde_json::to_value(AgentStatus::Disconnected).unwrap(),
        json!("disconnected")
    );
    assert_eq!(
        serde_json::to_value(AgentStatus::Unsupported).unwrap(),
        json!("unsupported")
    );
    assert_eq!(
        serde_json::to_value(SettingsSection::McpServers).unwrap(),
        json!("mcpServers")
    );
    assert_eq!(
        serde_json::to_value(SettingsSection::CommonSettings).unwrap(),
        json!("commonSettings")
    );
}
