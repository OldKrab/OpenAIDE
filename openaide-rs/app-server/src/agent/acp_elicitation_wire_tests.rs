use agent_client_protocol::JsonRpcMessage;
use serde_json::json;

use super::ElicitationCreateRequest;

#[test]
fn dual_scope_is_preserved_for_semantic_rejection() {
    let parsed = ElicitationCreateRequest::parse_message(
        "elicitation/create",
        &json!({
            "sessionId": "session-1",
            "requestId": 7,
            "mode": "form",
            "message": "Choose",
            "requestedSchema": { "type": "object", "properties": {} }
        }),
    )
    .unwrap();

    assert!(parsed.session_id.is_some());
    assert!(parsed.request_id.is_some());
}

#[test]
fn titled_choice_descriptions_survive_the_compatibility_seam() {
    let parsed = ElicitationCreateRequest::parse_message(
        "elicitation/create",
        &json!({
            "sessionId": "session-1",
            "mode": "form",
            "message": "Choose",
            "requestedSchema": {
                "type": "object",
                "properties": { "strategy": { "type": "string", "oneOf": [
                    { "const": "safe", "title": "Safe", "description": "Small changes" }
                ] } }
            }
        }),
    )
    .unwrap();

    let serialized = serde_json::to_value(parsed).unwrap();
    assert_eq!(
        serialized["requestedSchema"]["properties"]["strategy"]["oneOf"][0]["description"],
        "Small changes"
    );
}

#[test]
fn codex_question_metadata_survives_the_compatibility_seam() {
    let parsed = ElicitationCreateRequest::parse_message(
        "elicitation/create",
        &json!({
            "sessionId": "session-1",
            "toolCallId": "call-1",
            "mode": "form",
            "message": "Would you rather watch a sunrise or a sunset?",
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "random_preference": {
                        "type": "string",
                        "title": "Preference",
                        "description": "Would you rather watch a sunrise or a sunset?",
                        "oneOf": [
                            { "const": "Sunset", "title": "Sunset", "description": "Evening" },
                            { "const": "Sunrise", "title": "Sunrise", "description": "Morning" }
                        ],
                        "_meta": { "codex": { "isOther": true, "isSecret": false } }
                    },
                    "random_preference__other": {
                        "type": "string",
                        "title": "Other",
                        "description": "Type your own answer.",
                        "_meta": {
                            "codex": {
                                "questionId": "random_preference",
                                "isOtherAnswer": true,
                                "isSecret": false
                            }
                        }
                    }
                },
                "required": []
            },
            "_meta": { "codex": { "autoResolutionMs": null } }
        }),
    )
    .unwrap();

    let serialized = serde_json::to_value(parsed).unwrap();
    assert_eq!(
        serialized["requestedSchema"]["properties"]["random_preference"]["_meta"]["codex"]
            ["isOther"],
        true
    );
    assert_eq!(
        serialized["requestedSchema"]["properties"]["random_preference__other"]["_meta"]["codex"]
            ["questionId"],
        "random_preference"
    );
}
