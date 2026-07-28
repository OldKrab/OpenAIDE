use std::collections::BTreeMap;

use openaide_app_server_protocol::settings::{
    McpCreateServerParams, McpDeleteServerParams, McpGetServerDetailsParams,
    McpServerConfiguration, McpServerDefinition, McpServerScope, McpSetServerEnabledParams,
    McpUpdateServerParams, SettingsMcpServerStatus, SettingsMcpServersParams,
    SettingsProjectionAvailability,
};

use super::*;
use crate::storage::Store;

#[test]
fn mcp_workflow_lists_details_and_mutates_durable_definitions() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");
    let service = McpServersSettingsService::new(store);
    let mut server = server();

    let created = service
        .create_mcp_server(McpCreateServerParams {
            server: server.clone(),
        })
        .expect("create server");
    assert_eq!(created.server_id, "mcp-files");
    assert_eq!(
        created.servers.availability,
        SettingsProjectionAvailability::Available
    );
    assert_eq!(
        created.servers.servers[0].status,
        SettingsMcpServerStatus::Configured
    );

    assert_eq!(
        service
            .mcp_server_details(McpGetServerDetailsParams {
                id: "mcp-files".to_string(),
            })
            .expect("read details")
            .server,
        server
    );

    server.label = "Local files".to_string();
    service
        .update_mcp_server(McpUpdateServerParams {
            server: server.clone(),
            expected_secret_names: vec!["TOKEN".to_string()],
        })
        .expect("update server");
    let disabled = service
        .set_mcp_server_enabled(McpSetServerEnabledParams {
            id: server.id.clone(),
            enabled: false,
        })
        .expect("disable server");
    assert_eq!(
        disabled.servers.servers[0].status,
        SettingsMcpServerStatus::Disabled
    );

    service
        .delete_mcp_server(McpDeleteServerParams {
            id: server.id,
            expected_secret_names: vec!["TOKEN".to_string()],
        })
        .expect("delete server");
    assert!(service
        .mcp_servers_settings(SettingsMcpServersParams {})
        .unwrap()
        .servers
        .is_empty());
}

fn server() -> McpServerDefinition {
    McpServerDefinition {
        id: "mcp-files".to_string(),
        label: "Filesystem".to_string(),
        description: Some("Approved local files".to_string()),
        enabled: true,
        scope: McpServerScope::Global,
        configuration: McpServerConfiguration::Stdio {
            command_line: "/usr/bin/npx filesystem".to_string(),
            command: "/usr/bin/npx".to_string(),
            args: vec!["filesystem".to_string()],
            env: BTreeMap::new(),
            secret_env: vec!["TOKEN".to_string()],
        },
    }
}
