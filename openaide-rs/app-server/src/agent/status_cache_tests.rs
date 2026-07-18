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
        logout_supported: false,
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

#[test]
fn authenticating_status_retains_the_selected_method_until_completion() {
    let cache = AgentStatusCache::default();

    cache
        .begin_authentication("codex", "browser-login", false)
        .unwrap();
    let authenticating = cache.snapshot("codex");
    assert_eq!(authenticating.status, AgentStatus::Authenticating);
    assert_eq!(
        authenticating.authenticating_method_id.as_deref(),
        Some("browser-login")
    );

    cache.record_authentication_success("codex");
    assert_eq!(cache.snapshot("codex").authenticating_method_id, None);
}

#[test]
fn only_the_pending_terminal_method_can_continue_authentication() {
    let cache = AgentStatusCache::default();
    cache
        .begin_authentication("codex", "terminal", false)
        .unwrap();

    assert!(matches!(
        cache.begin_authentication("codex", "other", false),
        Err(RuntimeError::Conflict(_))
    ));
    cache
        .begin_authentication("codex", "terminal", true)
        .unwrap();
}

#[test]
fn failed_authentication_restores_the_status_that_required_or_started_it() {
    let connected = AgentStatusCache::default();
    connected.record_probe_success(&AgentProbeResult {
        agent_id: "codex".to_string(),
        status: AgentProbeStatus::Ready,
        protocol_version: "1".to_string(),
        implementation_name: None,
        implementation_version: None,
        capabilities: Vec::new(),
        typed_capabilities: AgentProbeCapabilities::default(),
        auth_methods: Vec::new(),
        logout_supported: false,
    });
    connected
        .begin_authentication("codex", "api-key", false)
        .unwrap();
    connected.record_authentication_error(
        "codex",
        &RuntimeError::Internal("Agent auth failed".to_string()),
    );
    assert_eq!(connected.snapshot("codex").status, AgentStatus::Connected);

    let required = AgentStatusCache::default();
    required.record_probe_error(
        "codex",
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );
    required
        .begin_authentication("codex", "api-key", false)
        .unwrap();
    required.record_authentication_error(
        "codex",
        &RuntimeError::Internal("Agent auth failed".to_string()),
    );
    assert_eq!(required.snapshot("codex").status, AgentStatus::AuthRequired);
}
