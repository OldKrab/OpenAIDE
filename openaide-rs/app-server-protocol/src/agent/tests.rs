use super::*;
use crate::ids::AgentId;

#[test]
fn probe_params_are_camel_case() {
    let value = serde_json::to_value(AgentProbeParams {
        agent_id: AgentId::from("codex"),
    })
    .unwrap();

    assert_eq!(value["agentId"], "codex");
}

#[test]
fn replacement_cleanup_exposes_only_removed_secret_names() {
    let value = serde_json::to_value(AgentReplaceCustomCleanup {
        removed_catalog_record: true,
        removed_cached_status: false,
        removed_settings_overlay: false,
        removed_secret_env: vec!["TOKEN".to_string()],
        history_policy: AgentReplaceCustomHistoryPolicy::PreserveHistoricalTasks,
    })
    .unwrap();

    assert_eq!(value["removedSecretEnv"], serde_json::json!(["TOKEN"]));
    assert!(value.get("secretValues").is_none());
}
