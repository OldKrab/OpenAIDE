use super::*;

/// The App Server and older devices read these messages, so their JSON shape must stay put.
#[test]
fn pairing_messages_keep_their_json_shape() {
    let request = InviteRequest {
        secret: "ABC".to_string(),
        name: "Phone".to_string(),
        model: None,
    };
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        serde_json::json!({ "secret": "ABC", "name": "Phone" })
    );

    let refused: PairingResult = serde_json::from_str(r#"{"trusted":false}"#).unwrap();
    assert_eq!(
        refused,
        PairingResult {
            trusted: false,
            server_name: None
        }
    );
    let trusted = PairingResult {
        trusted: true,
        server_name: Some("desk".to_string()),
    };
    assert_eq!(
        serde_json::to_value(&trusted).unwrap(),
        serde_json::json!({ "trusted": true, "serverName": "desk" })
    );
}

#[test]
fn a_secret_that_is_not_base32_is_refused() {
    let request = InviteRequest {
        secret: "not base32!".to_string(),
        name: "Phone".to_string(),
        model: None,
    };

    assert_eq!(request.secret_bytes(), None);
}
