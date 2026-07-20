use crate::agent::acp_schema::{SessionConfigOption, SetSessionConfigOptionResponse};

use super::normalize_config_options;

#[test]
fn normalizes_mixed_select_and_boolean_options_in_agent_order() {
    let catalog = normalize_config_options(
        "codex",
        vec![
            SessionConfigOption::boolean("brave_mode", "Brave Mode", true)
                .description("Skip confirmation prompts"),
            SessionConfigOption::select(
                "model",
                "Model",
                "gpt-5",
                vec![crate::agent::acp_schema::SessionConfigSelectOption::new(
                    "gpt-5", "GPT-5",
                )],
            ),
        ],
    );

    assert_eq!(catalog.options.len(), 2);
    assert_eq!(catalog.options[0].id, "brave_mode");
    assert_eq!(catalog.options[1].id, "model");
    assert_eq!(
        serde_json::to_value(&catalog.options[0]).unwrap(),
        serde_json::json!({
            "id": "brave_mode",
            "label": "Brave Mode",
            "description": "Skip confirmation prompts",
            "kind": "boolean",
            "current_value": { "type": "boolean", "value": true },
            "values": [],
        }),
    );
}

#[test]
fn malformed_or_unknown_option_kinds_are_ignored_without_losing_valid_options() {
    let response: SetSessionConfigOptionResponse = serde_json::from_value(serde_json::json!({
        "configOptions": [
            {
                "id": "future",
                "name": "Future control",
                "type": "slider",
                "currentValue": 3
            },
            {
                "id": "brave_mode",
                "name": "Brave Mode",
                "type": "boolean",
                "currentValue": false
            }
        ]
    }))
    .unwrap();

    let catalog = normalize_config_options("codex", response.config_options);

    assert_eq!(catalog.options.len(), 1);
    assert_eq!(catalog.options[0].id, "brave_mode");
    assert_eq!(catalog.options[0].current_value.as_bool(), Some(false));
}
