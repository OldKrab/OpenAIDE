use serde_json::json;

use super::{TaskSendCapabilitySnapshot, TaskSendCapabilityState};

#[test]
fn send_capability_contains_readiness_without_inventing_text_requirements() {
    let capability = TaskSendCapabilitySnapshot {
        state: TaskSendCapabilityState::Ready,
        blockers: Vec::new(),
    };

    assert_eq!(
        serde_json::to_value(capability).unwrap(),
        json!({ "state": "ready" })
    );
}
