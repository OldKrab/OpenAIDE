use crate::protocol::model::ConfigOptionCurrentValue;

use super::acp_config_value;

#[test]
fn serializes_boolean_config_values_for_the_acp_wire() {
    let value = acp_config_value(ConfigOptionCurrentValue::boolean(false));

    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({ "type": "boolean", "value": false }),
    );
}

#[test]
fn serializes_select_config_values_for_the_acp_wire() {
    let value = acp_config_value(ConfigOptionCurrentValue::id("gpt-5"));

    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({ "value": "gpt-5" }),
    );
}
