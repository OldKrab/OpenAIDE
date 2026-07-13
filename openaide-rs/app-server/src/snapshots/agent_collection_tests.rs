use super::*;
use crate::agent::registry::{AgentSourceKind, CODEX_AGENT_ID, OPENCODE_AGENT_ID};
use openaide_app_server_protocol::snapshot::AgentStatus;

#[test]
fn collection_preserves_registry_order_without_inventing_a_default() {
    let snapshot = collection_from_registry_summaries(vec![
        summary(OPENCODE_AGENT_ID, "OpenCode"),
        summary(CODEX_AGENT_ID, "Codex"),
    ]);

    assert_eq!(
        snapshot.agents[0].agent_id,
        AgentId::from(OPENCODE_AGENT_ID)
    );
    assert_eq!(snapshot.agents[0].status, AgentStatus::Disconnected);
}

#[test]
fn collection_uses_cached_agent_statuses() {
    let statuses = AgentStatusCache::default();
    statuses.record_probe_error(
        OPENCODE_AGENT_ID,
        &crate::protocol::errors::RuntimeError::AuthRequired("Authentication required".to_string()),
    );

    let snapshot = collection_from_registry_summaries_with_statuses(
        vec![
            summary(CODEX_AGENT_ID, "Codex"),
            summary(OPENCODE_AGENT_ID, "OpenCode"),
        ],
        &statuses,
    );

    assert_eq!(snapshot.agents[0].status, AgentStatus::Disconnected);
    assert_eq!(snapshot.agents[1].status, AgentStatus::AuthRequired);
}

fn summary(id: &str, label: &str) -> AgentDefinitionSummary {
    AgentDefinitionSummary {
        id: id.to_string(),
        label: label.to_string(),
        source_kind: AgentSourceKind::BuiltIn,
    }
}
