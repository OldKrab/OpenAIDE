use serde_json::json;

use super::*;
use crate::client::SettingsSection;

#[test]
fn client_snapshot_carries_the_global_state_and_omits_an_absent_active_task() {
    let snapshot = ClientSnapshot {
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
        global: crate::global_state::GlobalState {
            projects: ProjectCollectionSnapshot {
                projects: Vec::new(),
            },
            worktrees: Vec::new(),
            agents: AgentCollectionSnapshot { agents: Vec::new() },
            settings: SettingsSnapshot {
                sections: Vec::new(),
                preferences: None,
                runtime: None,
            },
            new_task_defaults: Default::default(),
            devices: crate::devices::DeviceCollectionSnapshot {
                remote_access: crate::devices::RemoteAccessState::Off,
                server_name: "computer".to_string(),
                server_key: None,
                own_relay: None,
                devices: Vec::new(),
            },
            terminals: crate::terminals::TerminalCollectionSnapshot {
                terminals: Vec::new(),
            },
        },
        active_task: None,
    };

    let value = serde_json::to_value(snapshot).unwrap();

    assert!(value.get("cursor").is_none());
    assert_eq!(value["global"]["projects"]["projects"], json!([]));
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
                    preview_image_id: None,
                },
            },
        ],
        timing: None,
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

#[test]
fn protocol_version_accepts_same_major_clients_up_to_its_minor() {
    let server = ProtocolVersion { major: 2, minor: 1 };

    assert!(server.accepts_client(ProtocolVersion { major: 2, minor: 0 }));
    assert!(server.accepts_client(ProtocolVersion { major: 2, minor: 1 }));
    assert!(!server.accepts_client(ProtocolVersion { major: 2, minor: 2 }));
    assert!(!server.accepts_client(ProtocolVersion { major: 1, minor: 0 }));
    assert!(!server.accepts_client(ProtocolVersion { major: 3, minor: 0 }));
}

#[test]
fn protocol_version_round_trips_as_text_and_reads_a_bare_major() {
    let version = ProtocolVersion { major: 2, minor: 1 };

    assert_eq!(version.to_string(), "2.1");
    assert_eq!("2.1".parse(), Ok(version));
    assert_eq!("2".parse(), Ok(ProtocolVersion { major: 2, minor: 0 }));
    assert!("two".parse::<ProtocolVersion>().is_err());
    assert!("2.x".parse::<ProtocolVersion>().is_err());
}
