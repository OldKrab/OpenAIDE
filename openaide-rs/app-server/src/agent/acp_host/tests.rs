use crate::protocol::host::HostBridge;

use super::initialize_request;

#[test]
fn form_elicitation_is_advertised_without_shell_host_capabilities() {
    let value = serde_json::to_value(initialize_request(&HostBridge::disabled())).unwrap();

    assert_eq!(
        value["clientCapabilities"]["elicitation"]["form"],
        serde_json::json!({})
    );
    assert_eq!(value["clientCapabilities"]["terminal"], false);
}
