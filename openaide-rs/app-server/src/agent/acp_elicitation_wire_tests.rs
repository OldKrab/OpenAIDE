use agent_client_protocol::JsonRpcMessage;
use serde_json::json;

use super::ElicitationCreateRequest;

#[test]
fn url_elicitation_preserves_the_external_destination_and_identity() {
    let parsed = ElicitationCreateRequest::parse_message(
        "elicitation/create",
        &json!({
            "requestId": 7,
            "mode": "url",
            "message": "Sign in to ChatGPT and enter this code: ABCD-EFGH",
            "elicitationId": "login-1",
            "url": "https://auth.openai.com/device"
        }),
    )
    .unwrap();

    let serialized = serde_json::to_value(parsed).unwrap();
    assert_eq!(serialized["elicitationId"], "login-1");
    assert_eq!(serialized["url"], "https://auth.openai.com/device");
}

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

/// Claude Code's AskUserQuestion forwards an option's `preview` under the option's `_meta`, and
/// ACP reserves `_meta` on the schema object and on every enum option. Rejecting either fails the
/// whole question before the user sees it.
#[test]
fn option_and_schema_meta_survive_the_compatibility_seam() {
    let option_meta = json!({ "_claude/askUserQuestionOption": { "preview": "┌──┐\n│TV│\n└──┘" } });
    let parsed = ElicitationCreateRequest::parse_message(
        "elicitation/create",
        &json!({
            "sessionId": "session-1",
            "toolCallId": "toolu-1",
            "mode": "form",
            "message": "Which side?",
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "question_0": { "type": "string", "title": "Outline", "oneOf": [
                        { "const": "East", "title": "East", "_meta": option_meta.clone() }
                    ] },
                    "question_1": { "type": "array", "items": { "anyOf": [
                        { "const": "West", "title": "West", "_meta": option_meta.clone() }
                    ] } }
                },
                "_meta": { "vendor": { "version": 1 } }
            }
        }),
    )
    .unwrap();

    let serialized = serde_json::to_value(parsed).unwrap();
    let schema = &serialized["requestedSchema"];
    assert_eq!(
        schema["properties"]["question_0"]["oneOf"][0]["_meta"],
        option_meta
    );
    assert_eq!(
        schema["properties"]["question_1"]["items"]["anyOf"][0]["_meta"],
        option_meta
    );
    assert_eq!(schema["_meta"]["vendor"]["version"], 1);
}
