use super::*;
use crate::protocol::model::{AgentProbeCapabilities, AgentProbeStatus};
use openaide_app_server_protocol::snapshot::{AgentSignInPhase, AgentStatus};

#[test]
fn probe_notifications_preserve_capability_authentication_and_other_agent_changes() {
    let (cache, updates) = AgentStatusCache::channel();
    let mut probe = AgentProbeResult {
        agent_id: "codex".to_string(),
        status: AgentProbeStatus::Ready,
        protocol_version: "fixture".to_string(),
        implementation_name: None,
        implementation_version: None,
        capabilities: Vec::new(),
        typed_capabilities: AgentProbeCapabilities::default(),
        auth_methods: Vec::new(),
        logout_supported: false,
    };
    cache.record_probe_success(&probe);
    assert!(updates.try_recv().is_ok());
    cache.record_probe_success(&probe);
    assert!(
        updates.try_recv().is_err(),
        "identical observations stay quiet"
    );

    probe.typed_capabilities.resume_sessions = true;
    cache.record_probe_success(&probe);
    assert!(
        updates.try_recv().is_ok(),
        "capability changes wake discovery"
    );
    assert!(cache.snapshot("codex").capabilities.resume_tasks);

    probe.auth_methods.push(AgentAuthMethodSummary {
        id: "fixture-sign-in".to_string(),
        label: "Fixture sign-in".to_string(),
        kind: "agent".to_string(),
        description: None,
        variables: Vec::new(),
        link: None,
        terminal_args: Vec::new(),
        terminal_env: Default::default(),
    });
    cache.record_probe_success(&probe);
    assert!(
        updates.try_recv().is_ok(),
        "new sign-in choices wake discovery"
    );
    let codex = cache.snapshot("codex");
    assert_eq!(codex.auth_methods, probe.auth_methods);

    cache.record_probe_error(
        "opencode",
        &RuntimeError::AuthRequired("Fixture".to_string()),
    );
    assert!(
        updates.try_recv().is_ok(),
        "another Agent remains independent"
    );
    assert_eq!(cache.snapshot("codex"), codex);
    assert_eq!(cache.snapshot("opencode").status, AgentStatus::AuthRequired);
    assert!(updates.try_recv().is_err());
}

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

fn five_hour_usage(used_percent: u8) -> crate::agent::events::AgentAccountLimitsChange {
    use crate::agent::events::{
        AgentAccountLimitUsage, AgentAccountLimitWindowId, AgentAccountLimitsChange,
    };
    AgentAccountLimitsChange::Usage {
        plan_label: Some("Max".to_string()),
        windows: vec![AgentAccountLimitUsage {
            window: AgentAccountLimitWindowId {
                kind: openaide_app_server_protocol::snapshot::AgentAccountLimitWindowKind::FiveHour,
                model_label: None,
            },
            used_percent,
            resets_at_ms: None,
        }],
    }
}

#[test]
fn account_limits_notify_only_when_the_visible_value_changes() {
    let (cache, updates) = AgentStatusCache::channel();
    assert_eq!(cache.account_limits("claude"), None);

    cache.record_account_limits("claude", five_hour_usage(40));
    assert!(updates.try_recv().is_ok());
    cache.record_account_limits("claude", five_hour_usage(40));
    assert!(
        updates.try_recv().is_err(),
        "an unchanged reading stays quiet"
    );

    cache.record_account_limits("claude", five_hour_usage(44));
    assert!(updates.try_recv().is_ok());
    let limits = cache.account_limits("claude").expect("limits");
    assert_eq!(limits.windows[0].used_percent, 44);
    assert_eq!(cache.account_limits("codex"), None);
}

#[test]
fn account_limits_survive_status_replacement_and_end_with_the_account() {
    let cache = AgentStatusCache::default();
    cache.record_account_limits("claude", five_hour_usage(40));

    cache.record_probe_error(
        "claude",
        &RuntimeError::CapabilityMissing("agent_probe:claude".to_string()),
    );
    cache.record_connected("claude");
    assert!(cache.account_limits("claude").is_some());

    cache.record_logout_success("claude");
    assert_eq!(cache.account_limits("claude"), None);

    cache.record_account_limits("claude", five_hour_usage(40));
    assert!(cache.clear("claude"));
    assert_eq!(cache.account_limits("claude"), None);
}

#[test]
fn an_on_demand_read_is_claimed_once_per_interval_and_a_reading_counts_as_fresh() {
    let cache = AgentStatusCache::default();
    // timing: data — the minimum gap between reads, compared against elapsed time and never waited on.
    let interval = std::time::Duration::from_secs(60);

    assert!(cache.claim_account_limits_read("claude", interval));
    assert!(
        !cache.claim_account_limits_read("claude", interval),
        "a running or recent attempt answers the next caller"
    );
    assert!(cache.claim_account_limits_read("codex", interval));
    assert!(cache.claim_account_limits_read("claude", std::time::Duration::ZERO));

    let cache = AgentStatusCache::default();
    cache.record_account_limits("claude", five_hour_usage(40));
    assert!(
        !cache.claim_account_limits_read("claude", interval),
        "a reading a turn just delivered needs no second read"
    );
    // A new account must not inherit the previous one's freshness.
    cache.record_logout_success("claude");
    assert!(cache.claim_account_limits_read("claude", interval));
}

#[test]
fn an_agent_that_refused_the_read_is_not_asked_again_until_it_is_replaced() {
    let cache = AgentStatusCache::default();
    cache.record_account_limits_unsupported("claude");
    assert!(!cache.claim_account_limits_read("claude", std::time::Duration::ZERO));

    cache.clear("claude");
    assert!(cache.claim_account_limits_read("claude", std::time::Duration::ZERO));
}

#[test]
fn a_recorder_files_pushed_limits_under_its_agent() {
    let cache = AgentStatusCache::default();
    cache
        .account_limits_recorder("claude")
        .record(five_hour_usage(12));
    assert_eq!(
        cache.account_limits("claude").expect("limits").windows[0].used_percent,
        12
    );
}
