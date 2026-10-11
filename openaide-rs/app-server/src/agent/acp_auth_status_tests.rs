use serde_json::json;

use super::reported_signed_in;

#[test]
fn only_the_known_signed_out_kind_reads_as_signed_out() {
    assert_eq!(
        reported_signed_in(&json!({ "authStatus": { "kind": "none" } })),
        Some(false)
    );
    for kind in ["account", "api_key", "gateway", "external", "added_later"] {
        assert_eq!(
            reported_signed_in(&json!({ "authStatus": { "kind": kind } })),
            Some(true)
        );
    }
}

#[test]
fn a_payload_without_a_kind_says_nothing() {
    assert_eq!(reported_signed_in(&json!({})), None);
    assert_eq!(reported_signed_in(&json!({ "authStatus": {} })), None);
    assert_eq!(
        reported_signed_in(&json!({ "authStatus": { "kind": 1 } })),
        None
    );
}
