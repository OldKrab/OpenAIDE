use super::*;
use crate::protocol::model::{AgentProbeCapabilities, AgentProbeStatus};
use openaide_app_server_protocol::snapshot::AgentStatus;

#[test]
fn successful_probe_records_connected_status_and_capabilities() {
    let cache = AgentStatusCache::default();

    cache.record_probe_success(&AgentProbeResult {
        agent_id: "codex".to_string(),
        status: AgentProbeStatus::Ready,
        protocol_version: "1".to_string(),
        implementation_name: None,
        implementation_version: None,
        capabilities: vec![
            "Basic sessions".to_string(),
            "Resume sessions".to_string(),
            "Delete sessions".to_string(),
        ],
        typed_capabilities: AgentProbeCapabilities {
            resume_sessions: true,
            delete_sessions: true,
        },
        auth_methods: Vec::new(),
    });

    let snapshot = cache.snapshot("codex");
    assert_eq!(snapshot.status, AgentStatus::Connected);
    assert!(snapshot.capabilities.resume_tasks);
    assert!(snapshot.capabilities.delete_native_sessions);
}

#[test]
fn failed_probe_records_user_visible_status() {
    let cache = AgentStatusCache::default();

    cache.record_probe_error(
        "codex",
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );

    assert_eq!(cache.snapshot("codex").status, AgentStatus::AuthRequired);
}

#[test]
fn missing_probe_capability_records_unsupported_status() {
    let cache = AgentStatusCache::default();

    cache.record_probe_error(
        "codex",
        &RuntimeError::CapabilityMissing("agent_probe:codex".to_string()),
    );

    assert_eq!(cache.snapshot("codex").status, AgentStatus::Unsupported);
}

#[test]
fn clear_removes_cached_status_and_capabilities() {
    let cache = AgentStatusCache::default();
    cache.record_probe_error(
        "codex",
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );

    assert!(cache.clear("codex"));
    assert!(!cache.clear("codex"));

    assert_eq!(cache.snapshot("codex"), AgentStatusSnapshot::default());
}
