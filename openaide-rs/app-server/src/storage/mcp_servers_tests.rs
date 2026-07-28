use std::collections::BTreeMap;

use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::settings::{
    McpServerConfiguration, McpServerDefinition, McpServerScope,
};

use crate::storage::Store;

#[test]
fn mcp_servers_round_trip_without_persisting_secret_values() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");
    let global = stdio_server("server-global", McpServerScope::Global);
    let project = http_server(
        "server-project",
        McpServerScope::Project {
            project_id: ProjectId::from("project-one"),
        },
    );

    store
        .create_mcp_server(global.clone())
        .expect("create global server");
    store
        .create_mcp_server(project.clone())
        .expect("create project server");
    drop(store);

    let reopened = Store::open(root.path().to_path_buf()).expect("reopen store");
    assert_eq!(
        reopened.read_mcp_servers().expect("read MCP servers"),
        vec![global, project]
    );
    let persisted = std::fs::read_to_string(root.path().join("settings/mcp_servers.json")).unwrap();
    assert!(!persisted.contains("secret-token"));
}

#[test]
fn mcp_server_mutations_guard_secret_cleanup_and_preserve_identity() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");
    let mut server = stdio_server("server-stable", McpServerScope::Global);
    store
        .create_mcp_server(server.clone())
        .expect("create MCP server");

    server.label = "Local files".to_string();
    let McpServerConfiguration::Stdio { secret_env, .. } = &mut server.configuration else {
        unreachable!("test server is stdio");
    };
    *secret_env = vec!["NEW_TOKEN".to_string()];
    assert!(store
        .update_mcp_server(server.clone(), &["STALE_TOKEN".to_string()])
        .is_err());
    store
        .update_mcp_server(server.clone(), &["TOKEN".to_string()])
        .expect("update with current secret names");

    let disabled = store
        .set_mcp_server_enabled("server-stable", false)
        .expect("disable server");
    assert_eq!(disabled.id, "server-stable");
    assert!(!disabled.enabled);

    assert!(store
        .delete_mcp_server("server-stable", &["TOKEN".to_string()])
        .is_err());
    let removed = store
        .delete_mcp_server("server-stable", &["NEW_TOKEN".to_string()])
        .expect("delete with current secret names");
    assert_eq!(removed, vec!["NEW_TOKEN"]);
    assert!(store.read_mcp_servers().unwrap().is_empty());
}

#[test]
fn effective_mcp_servers_apply_project_overrides_and_ignore_disabled_definitions() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");
    let global = stdio_server("global-files", McpServerScope::Global);
    let mut project = http_server(
        "project-files",
        McpServerScope::Project {
            project_id: ProjectId::from("project-one"),
        },
    );
    project.label = global.label.clone();
    project.enabled = true;
    let mut disabled = stdio_server("disabled", McpServerScope::Global);
    disabled.label = "Disabled".to_string();
    disabled.enabled = false;
    for server in [global, project.clone(), disabled] {
        store.create_mcp_server(server).unwrap();
    }

    assert_eq!(
        store
            .effective_mcp_servers(Some(&ProjectId::from("project-one")))
            .unwrap(),
        vec![project]
    );
    assert_eq!(
        store.effective_mcp_servers(None).unwrap()[0].id,
        "global-files"
    );
}

#[test]
fn mcp_server_names_are_unique_within_a_scope() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");
    let first = stdio_server("server-one", McpServerScope::Global);
    store.create_mcp_server(first).expect("create first server");

    let mut duplicate = http_server("server-two", McpServerScope::Global);
    duplicate.label = " filesystem ".to_string();
    assert!(store.create_mcp_server(duplicate).is_err());

    let mut project = http_server(
        "server-project",
        McpServerScope::Project {
            project_id: ProjectId::from("project-one"),
        },
    );
    project.label = "Filesystem".to_string();
    store
        .create_mcp_server(project)
        .expect("project override may reuse a global name");

    let mut renamed = stdio_server("server-three", McpServerScope::Global);
    renamed.label = "Database".to_string();
    store
        .create_mcp_server(renamed.clone())
        .expect("create another global server");
    renamed.label = "FILESYSTEM".to_string();
    assert!(store
        .update_mcp_server(renamed, &["TOKEN".to_string()])
        .is_err());
}

fn stdio_server(id: &str, scope: McpServerScope) -> McpServerDefinition {
    McpServerDefinition {
        id: id.to_string(),
        label: "Filesystem".to_string(),
        description: Some("Approved files".to_string()),
        enabled: true,
        scope,
        configuration: McpServerConfiguration::Stdio {
            command_line: "npx filesystem".to_string(),
            command: "/usr/bin/npx".to_string(),
            args: vec!["filesystem".to_string()],
            env: BTreeMap::from([("MODE".to_string(), "readonly".to_string())]),
            secret_env: vec!["TOKEN".to_string()],
        },
    }
}

fn http_server(id: &str, scope: McpServerScope) -> McpServerDefinition {
    McpServerDefinition {
        id: id.to_string(),
        label: "GitHub".to_string(),
        description: None,
        enabled: false,
        scope,
        configuration: McpServerConfiguration::Http {
            url: "https://example.invalid/mcp".to_string(),
            headers: BTreeMap::from([("Accept".to_string(), "application/json".to_string())]),
            secret_headers: vec!["Authorization".to_string()],
        },
    }
}
