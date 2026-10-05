use crate::protocol::host::HostBridge;

use super::{
    initialize_request, initialize_request_with_subagents, native_subagents_enabled_with_override,
};

#[test]
fn native_subagents_are_advertised_without_a_rollout_flag() {
    for override_value in [None, Some(""), Some("  "), Some("1"), Some(" TRUE ")] {
        let value = serde_json::to_value(initialize_request_with_subagents(
            &HostBridge::disabled(),
            native_subagents_enabled_with_override(override_value),
        ))
        .unwrap();
        assert_eq!(
            value["clientCapabilities"]["subagents"],
            serde_json::json!({})
        );
        assert_eq!(
            value["clientCapabilities"]["_meta"]["openaide"]["nativeSubagentSessions"],
            true
        );
    }
}

#[test]
fn native_subagent_rollback_omits_both_wire_capabilities() {
    for override_value in ["0", "false", " FALSE ", "invalid"] {
        let value = serde_json::to_value(initialize_request_with_subagents(
            &HostBridge::disabled(),
            native_subagents_enabled_with_override(Some(override_value)),
        ))
        .unwrap();
        assert!(value["clientCapabilities"].get("subagents").is_none());
        assert!(value["clientCapabilities"]["_meta"]
            .get("openaide")
            .is_none());
    }
}

#[test]
fn form_elicitation_is_advertised_without_shell_host_capabilities() {
    let value = serde_json::to_value(initialize_request(&HostBridge::disabled())).unwrap();

    assert_eq!(
        value["clientCapabilities"]["elicitation"]["form"],
        serde_json::json!({})
    );
    assert_eq!(value["clientCapabilities"]["terminal"], false);
    assert!(value["clientCapabilities"]["elicitation"]
        .get("url")
        .is_none());
    assert_eq!(value["clientCapabilities"]["auth"]["terminal"], false);
    assert_eq!(
        value["clientCapabilities"]["session"]["configOptions"]["boolean"],
        serde_json::json!({})
    );
    assert_eq!(
        value["clientCapabilities"]["_meta"]["parameterizedModelPicker"],
        true
    );
}

#[test]
fn session_compaction_is_advertised_so_agents_send_structured_updates() {
    for bridge in [HostBridge::disabled(), HostBridge::channel().0] {
        let value = serde_json::to_value(initialize_request(&bridge)).unwrap();

        assert_eq!(
            value["clientCapabilities"]["session"]["compaction"],
            serde_json::json!({})
        );
    }
}

#[test]
fn terminal_auth_is_advertised_when_the_app_shell_host_is_available() {
    let (bridge, _requests) = HostBridge::channel();
    let value = serde_json::to_value(initialize_request(&bridge)).unwrap();

    assert_eq!(value["clientCapabilities"]["auth"]["terminal"], true);
    assert_eq!(
        value["clientCapabilities"]["elicitation"]["url"],
        serde_json::json!({})
    );
}

#[test]
fn disabling_native_subagents_omits_the_canonical_and_sdk_bridge_capabilities() {
    let disabled = serde_json::to_value(initialize_request_with_subagents(
        &HostBridge::disabled(),
        false,
    ))
    .unwrap();
    let enabled = serde_json::to_value(initialize_request_with_subagents(
        &HostBridge::disabled(),
        true,
    ))
    .unwrap();

    assert!(disabled["clientCapabilities"].get("subagents").is_none());
    assert!(disabled["clientCapabilities"]["_meta"]
        .get("openaide")
        .is_none());
    assert_eq!(
        enabled["clientCapabilities"]["subagents"],
        serde_json::json!({})
    );
    assert_eq!(
        enabled["clientCapabilities"]["_meta"]["openaide"]["nativeSubagentSessions"],
        true
    );
}
