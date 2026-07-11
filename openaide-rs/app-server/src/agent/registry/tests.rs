use serde_json::json;

use super::*;

fn catalog_record(value: serde_json::Value) -> AgentCatalogRecord {
    serde_json::from_value(value).unwrap()
}

#[test]
fn registry_resolves_codex_launch_and_options_key() {
    let registry = AgentRegistry::default_built_ins();
    let codex = registry.require(CODEX_AGENT_ID).unwrap();

    assert_eq!(codex.id, CODEX_AGENT_ID);
    assert_eq!(codex.label(), CODEX_AGENT_LABEL);
    assert_eq!(codex.source_kind, AgentSourceKind::BuiltIn);
    assert_eq!(
        codex.options_request_key("/workspace/app"),
        "codex\0/workspace/app"
    );
}

#[test]
fn registry_resolves_opencode_as_builtin_agent() {
    let registry = AgentRegistry::default_built_ins();
    let opencode = registry.require(OPENCODE_AGENT_ID).unwrap();

    assert_eq!(opencode.id, OPENCODE_AGENT_ID);
    assert_eq!(opencode.label(), OPENCODE_AGENT_LABEL);
    assert_eq!(opencode.source_kind, AgentSourceKind::BuiltIn);
    assert_eq!(
        opencode.options_request_key("/workspace/app"),
        "opencode\0/workspace/app"
    );
}

#[test]
fn registry_summaries_are_stable_and_label_only() {
    let summaries = AgentRegistry::default_built_ins().summaries();

    assert_eq!(
        summaries
            .iter()
            .map(|agent| (agent.id.as_str(), agent.label.as_str(), agent.source_kind))
            .collect::<Vec<_>>(),
        vec![
            (CODEX_AGENT_ID, CODEX_AGENT_LABEL, AgentSourceKind::BuiltIn),
            (
                OPENCODE_AGENT_ID,
                OPENCODE_AGENT_LABEL,
                AgentSourceKind::BuiltIn
            ),
        ],
    );
}

#[test]
fn registry_rejects_unknown_agents_at_the_agent_seam() {
    let registry = AgentRegistry::default_built_ins();

    assert!(matches!(
        registry.require("custom").unwrap_err(),
        RuntimeError::CapabilityMissing(_)
    ));
}

#[test]
fn registry_builds_custom_stdio_agents_from_catalog_records() {
    let registry = AgentRegistry::from_agent_catalog(vec![
        catalog_record(json!({
            "id": "custom-agent",
            "label": "Custom Agent",
            "source_kind": "custom",
            "transport": "stdio",
            "command": "custom-acp",
            "args": ["--mode", "agent"],
            "env": { "CUSTOM_TOKEN": "redacted" },
            "secret_env": ["SECRET_TOKEN"]
        })),
        catalog_record(json!({
            "id": "disabled",
            "label": "Disabled",
            "source_kind": "custom",
            "enabled": false,
            "transport": "stdio",
            "command": "disabled-acp"
        })),
    ])
    .unwrap();

    let config = registry.require_acp_config("custom-agent").unwrap();
    assert_eq!(config.command, "custom-acp");
    assert_eq!(config.args, ["--mode", "agent"]);
    assert_eq!(config.secret_env, ["SECRET_TOKEN"]);
    assert!(registry.require("disabled").is_err());
}

#[test]
fn registry_uses_builtin_codex_launch_policy_for_catalog_codex_record() {
    let registry = AgentRegistry::from_agent_catalog(vec![catalog_record(json!({
        "id": CODEX_AGENT_ID,
        "label": CODEX_AGENT_LABEL,
        "source_kind": "built_in",
        "transport": "stdio",
        "command": "missing-codex-acp",
        "args": ["ignored"],
        "env": { "IGNORED": "1" }
    }))])
    .unwrap();

    let config = registry.require_acp_config(CODEX_AGENT_ID).unwrap();

    assert_ne!(config.command, "missing-codex-acp");
    assert_ne!(config.args, ["ignored"]);
    assert!(config.env.is_empty());
}

#[test]
fn registry_uses_builtin_opencode_launch_policy_for_catalog_opencode_record() {
    let registry = AgentRegistry::from_agent_catalog(vec![catalog_record(json!({
        "id": OPENCODE_AGENT_ID,
        "label": OPENCODE_AGENT_LABEL,
        "source_kind": "built_in",
        "transport": "stdio",
        "command": "missing-opencode",
        "args": ["ignored"],
        "env": { "IGNORED": "1" }
    }))])
    .unwrap();

    let config = registry.require_acp_config(OPENCODE_AGENT_ID).unwrap();

    assert_ne!(config.command, "missing-opencode");
    assert_ne!(config.args, ["ignored"]);
    assert!(config.args.iter().any(|arg| arg == "acp"));
    assert!(config.env.is_empty());
}

#[test]
fn registry_overlay_starts_with_builtins_and_applies_stored_overrides() {
    let registry = AgentRegistry::from_catalog_overlay(vec![
        catalog_record(json!({
            "id": CODEX_AGENT_ID,
            "enabled": false
        })),
        catalog_record(json!({
            "id": "custom-agent",
            "label": "Custom Agent",
            "source_kind": "custom",
            "transport": "stdio",
            "command": "custom-acp"
        })),
    ])
    .unwrap();

    assert!(registry.require(CODEX_AGENT_ID).is_err());
    assert!(registry.require(OPENCODE_AGENT_ID).is_ok());
    assert!(registry.require("custom-agent").is_ok());
}
