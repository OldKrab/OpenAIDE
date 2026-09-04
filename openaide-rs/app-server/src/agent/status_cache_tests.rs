use super::*;
use crate::protocol::model::{AgentProbeCapabilities, AgentProbeStatus};
use openaide_app_server_protocol::snapshot::{AgentSignInPhase, AgentStatus};

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
            fork_sessions: false,
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
fn auth_required_keeps_previously_advertised_authentication_methods() {
    let cache = AgentStatusCache::default();
    cache.record_probe_success(&AgentProbeResult {
        agent_id: "codex".to_string(),
        status: AgentProbeStatus::Ready,
        protocol_version: "1".to_string(),
        implementation_name: None,
        implementation_version: None,
        capabilities: Vec::new(),
        typed_capabilities: AgentProbeCapabilities::default(),
        auth_methods: vec![crate::protocol::model::AgentAuthMethodSummary {
            id: "chat-gpt".to_string(),
            label: "ChatGPT".to_string(),
            kind: "agent".to_string(),
            description: None,
            variables: Vec::new(),
            link: None,
            terminal_args: Vec::new(),
            terminal_env: Default::default(),
        }],
        logout_supported: false,
    });

    cache.record_probe_error(
        "codex",
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );

    let snapshot = cache.snapshot("codex");
    assert_eq!(snapshot.status, AgentStatus::AuthRequired);
    assert_eq!(snapshot.auth_methods.len(), 1);
    assert_eq!(snapshot.auth_methods[0].id, "chat-gpt");
}

#[test]
fn managed_integration_installation_is_visible_until_agent_launch_begins() {
    let cache = AgentStatusCache::default();

    let previous = cache.begin_installation("codex");
    assert_eq!(cache.snapshot("codex").status, AgentStatus::Installing);

    cache.complete_installation("codex", previous);
    assert_eq!(cache.snapshot("codex").status, AgentStatus::Launching);
}

#[test]
fn successful_session_records_connected_without_replacing_authenticating() {
    let cache = AgentStatusCache::default();
    cache.record_connected("codex");
    assert_eq!(cache.snapshot("codex").status, AgentStatus::Connected);

    cache
        .begin_authentication("codex", "browser-login", false)
        .unwrap();
    cache.record_connected("codex");
    assert_eq!(cache.snapshot("codex").status, AgentStatus::Authenticating);
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
        authenticating.running_sign_in_method_id(),
        Some("browser-login")
    );
    assert_eq!(
        authenticating.sign_in.as_ref().map(|flow| flow.phase),
        Some(AgentSignInPhase::Starting)
    );

    cache.record_authentication_success("codex");
    assert_eq!(cache.snapshot("codex").sign_in, None);
}

#[test]
fn sign_in_flow_publishes_the_agent_supplied_url_and_hint_while_running() {
    let cache = AgentStatusCache::default();
    cache
        .begin_authentication("codex", "chat-gpt-device-code", false)
        .unwrap();

    assert!(cache.record_sign_in_awaiting_user(
        "codex",
        "https://auth.example/device".to_string(),
        Some("Enter code ABCD-EFGH".to_string()),
    ));
    let flow = cache.snapshot("codex").sign_in.unwrap();
    assert_eq!(flow.phase, AgentSignInPhase::AwaitingUser);
    assert_eq!(flow.url.as_deref(), Some("https://auth.example/device"));
    assert_eq!(flow.hint.as_deref(), Some("Enter code ABCD-EFGH"));

    // A URL arriving after cancellation must not resurrect the flow.
    cache.record_authentication_error(
        "codex",
        &RuntimeError::NotReady("cancelled".to_string()),
        None,
    );
    assert!(!cache.record_sign_in_awaiting_user(
        "codex",
        "https://auth.example/device".to_string(),
        None,
    ));
    assert_eq!(cache.snapshot("codex").sign_in, None);
}

#[test]
fn failed_sign_in_stays_visible_until_dismissed_or_restarted() {
    let cache = AgentStatusCache::default();
    cache
        .begin_authentication("codex", "api-key", false)
        .unwrap();
    cache.record_authentication_error(
        "codex",
        &RuntimeError::Internal("agent said no".to_string()),
        Some("Codex could not sign in with API Key.".to_string()),
    );

    let flow = cache.snapshot("codex").sign_in.unwrap();
    assert_eq!(flow.phase, AgentSignInPhase::Failed);
    assert_eq!(flow.method_id, "api-key");
    assert_eq!(
        flow.failure.as_deref(),
        Some("Codex could not sign in with API Key.")
    );
    assert_eq!(cache.snapshot("codex").running_sign_in_method_id(), None);

    // A probe failure keeps the user's failed attempt visible.
    cache.record_probe_error(
        "codex",
        &RuntimeError::AuthRequired("Authentication required".to_string()),
    );
    assert_eq!(
        cache.snapshot("codex").sign_in.map(|flow| flow.phase),
        Some(AgentSignInPhase::Failed)
    );

    // Starting another flow replaces it.
    cache
        .begin_authentication("codex", "chat-gpt-device-code", false)
        .unwrap();
    assert_eq!(
        cache.snapshot("codex").sign_in.map(|flow| flow.phase),
        Some(AgentSignInPhase::Starting)
    );
    assert!(!cache.dismiss_failed_sign_in("codex"));

    cache.record_authentication_error(
        "codex",
        &RuntimeError::Internal("again".to_string()),
        Some("failed again".to_string()),
    );
    assert!(cache.dismiss_failed_sign_in("codex"));
    assert_eq!(cache.snapshot("codex").sign_in, None);
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
        None,
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
        None,
    );
    assert_eq!(required.snapshot("codex").status, AgentStatus::AuthRequired);
}
