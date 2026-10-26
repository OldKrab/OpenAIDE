use serde_json::json;

use super::subscription_diagnostic_fields;
use crate::protocol_edge::messages::InboundProtocolMessage;

fn request(method: &str, params: serde_json::Value) -> InboundProtocolMessage {
    InboundProtocolMessage::ClientRequest {
        id: "1".to_string(),
        method: method.to_string(),
        params,
        meta: Default::default(),
    }
}

#[test]
fn a_subscription_is_logged_by_its_scope_kind_and_task() {
    assert_eq!(
        subscription_diagnostic_fields(&request(
            "state/subscribe",
            json!({ "scope": { "kind": "task", "taskId": "task_1" } }),
        )),
        (Some("task".to_string()), Some("task_1".to_string()))
    );
    assert_eq!(
        subscription_diagnostic_fields(&request(
            "state/unsubscribe",
            json!({ "scope": { "kind": "taskNavigation" } }),
        )),
        (Some("taskNavigation".to_string()), None)
    );
    // Other methods and params that are not a scope name nothing.
    assert_eq!(
        subscription_diagnostic_fields(&request(
            "task/open",
            json!({ "scope": { "kind": "task", "taskId": "task_1" } }),
        )),
        (None, None)
    );
    assert_eq!(
        subscription_diagnostic_fields(&request("state/subscribe", json!({ "scope": 3 }))),
        (None, None)
    );
}
