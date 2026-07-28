use std::collections::BTreeMap;

use openaide_app_server_protocol::settings::{
    McpServerConfiguration, McpServerDefinition, McpServerScope,
};

use crate::agent::acp_schema::{McpCapabilities, McpServer};

use super::*;

#[test]
fn mcp_projection_merges_secrets_and_filters_unsupported_transports() {
    let definitions = vec![
        McpServerDefinition {
            id: "stdio".to_string(),
            label: "Filesystem".to_string(),
            description: None,
            enabled: true,
            scope: McpServerScope::Global,
            configuration: McpServerConfiguration::Stdio {
                command_line: "/usr/bin/mcp".to_string(),
                command: "/usr/bin/mcp".to_string(),
                args: vec!["serve".to_string()],
                env: BTreeMap::from([("MODE".to_string(), "read".to_string())]),
                secret_env: vec!["TOKEN".to_string()],
            },
        },
        McpServerDefinition {
            id: "http".to_string(),
            label: "Remote".to_string(),
            description: None,
            enabled: true,
            scope: McpServerScope::Global,
            configuration: McpServerConfiguration::Http {
                url: "https://example.invalid/mcp".to_string(),
                headers: BTreeMap::new(),
                secret_headers: vec!["Authorization".to_string()],
            },
        },
    ];

    let projected = project_mcp_servers(
        definitions,
        &McpCapabilities::new(),
        |server_id, kind, name| Ok(format!("{server_id}:{kind}:{name}")),
    )
    .expect("project MCP servers");

    assert_eq!(projected.len(), 1);
    let McpServer::Stdio(stdio) = &projected[0] else {
        panic!("stdio is always supported");
    };
    assert_eq!(stdio.name, "Filesystem");
    assert!(stdio
        .env
        .iter()
        .any(|item| item.name == "TOKEN" && item.value == "stdio:env:TOKEN"));
}
