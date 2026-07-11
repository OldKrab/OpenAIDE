use serde_json::json;

use super::*;
use crate::agent::registry::{CODEX_AGENT_ID, OPENCODE_AGENT_ID};

fn catalog_record(value: serde_json::Value) -> AgentCatalogRecord {
    serde_json::from_value(value).unwrap()
}

#[test]
fn missing_catalog_loads_builtin_registry() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentCatalogStore::new(Store::open(dir.path().to_path_buf()).unwrap());

    let registry = store.registry().unwrap();

    assert!(registry.require(CODEX_AGENT_ID).is_ok());
    assert!(registry.require(OPENCODE_AGENT_ID).is_ok());
}

#[test]
fn stored_catalog_can_add_custom_agents_and_disable_builtins() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentCatalogStore::new(Store::open(dir.path().to_path_buf()).unwrap());
    let records = vec![
        catalog_record(json!({
            "id": CODEX_AGENT_ID,
            "enabled": false
        })),
        catalog_record(json!({
            "id": "custom.local",
            "label": "Local Agent",
            "source_kind": "custom",
            "transport": "stdio",
            "command": "local-agent",
            "args": ["acp"],
            "secret_env": ["LOCAL_TOKEN"]
        })),
    ];

    store.save_records(&records).unwrap();
    let registry = store.registry().unwrap();

    assert!(registry.require(CODEX_AGENT_ID).is_err());
    assert!(registry.require(OPENCODE_AGENT_ID).is_ok());
    let config = registry.require_acp_config("custom.local").unwrap();
    assert_eq!(config.command, "local-agent");
    assert_eq!(config.args, ["acp"]);
    assert_eq!(config.secret_env, ["LOCAL_TOKEN"]);
}

#[test]
fn save_custom_replaces_existing_custom_agent_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentCatalogStore::new(Store::open(dir.path().to_path_buf()).unwrap());
    store
        .save_custom(catalog_record(json!({
            "id": "custom.local",
            "label": "Local Agent",
            "source_kind": "custom",
            "transport": "stdio",
            "command": "old-agent",
            "command_line": "old-agent"
        })))
        .unwrap();

    let registry = store
        .save_custom(catalog_record(json!({
            "id": "custom.local",
            "label": "Renamed Agent",
            "source_kind": "custom",
            "transport": "stdio",
            "command": "new-agent",
            "command_line": "new-agent --stdio",
            "args": ["--stdio"]
        })))
        .unwrap();

    let records = store.load_records().unwrap();
    assert_eq!(records.len(), 1);
    let config = registry.require_acp_config("custom.local").unwrap();
    assert_eq!(config.command, "new-agent");
    assert_eq!(config.args, ["--stdio"]);
}

#[test]
fn unsupported_catalog_schema_is_a_storage_error() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentCatalogStore::new(Store::open(dir.path().to_path_buf()).unwrap());
    std::fs::write(
        store.catalog_path(),
        serde_json::to_vec(&json!({
            "schemaVersion": 999,
            "records": []
        }))
        .unwrap(),
    )
    .unwrap();

    let error = store.registry().unwrap_err();

    assert!(matches!(error, RuntimeError::Storage(_)));
}
