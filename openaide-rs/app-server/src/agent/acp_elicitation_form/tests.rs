use std::collections::BTreeMap;

use openaide_app_server_protocol::server_requests::{
    QuestionField, QuestionRequestResponse, QuestionValue,
};

use crate::agent::acp_elicitation_wire::{
    ElicitationSchema, ObjectType, PropertySchema, StringFormat,
};

use super::{normalize_form, validate_product_response};

#[test]
fn full_restricted_schema_normalizes_without_raw_acp_payloads() {
    let schema = ElicitationSchema {
        type_: ObjectType::Object,
        title: None,
        description: None,
        required: vec!["name".to_string()],
        properties: BTreeMap::from([
            (
                "name".to_string(),
                PropertySchema::String {
                    title: Some("Name".to_string()),
                    description: Some("Identifier".to_string()),
                    min_length: Some(1),
                    max_length: Some(64),
                    pattern: Some("^[a-z]+$".to_string()),
                    format: None,
                    default: Some("project".to_string()),
                    enum_values: None,
                    one_of: None,
                },
            ),
            (
                "ratio".to_string(),
                PropertySchema::Number {
                    title: None,
                    description: None,
                    minimum: Some(0.0),
                    maximum: Some(1.0),
                    default: Some(0.5),
                },
            ),
            (
                "count".to_string(),
                PropertySchema::Integer {
                    title: None,
                    description: None,
                    minimum: Some(1),
                    maximum: Some(10),
                    default: Some(2),
                },
            ),
            (
                "enabled".to_string(),
                PropertySchema::Boolean {
                    title: None,
                    description: None,
                    default: Some(true),
                },
            ),
            (
                "email".to_string(),
                PropertySchema::String {
                    title: None,
                    description: None,
                    min_length: None,
                    max_length: None,
                    pattern: None,
                    format: Some(StringFormat::Email),
                    default: None,
                    enum_values: None,
                    one_of: None,
                },
            ),
        ]),
    };

    let form = normalize_form("Configure the project".to_string(), schema).unwrap();

    assert_eq!(form.fields.len(), 5);
    assert!(form.fields.iter().any(|field| matches!(field,
        QuestionField::String { key, required: true, .. } if key == "name")));
    assert!(form.fields.iter().any(|field| matches!(field,
        QuestionField::Number { key, default: Some(value), .. } if key == "ratio" && *value == 0.5)));
}

#[test]
fn response_validation_rejects_wrong_types_unknown_fields_and_constraints() {
    let schema = ElicitationSchema {
        type_: ObjectType::Object,
        title: None,
        description: None,
        required: vec!["name".to_string()],
        properties: BTreeMap::from([(
            "name".to_string(),
            PropertySchema::String {
                title: None,
                description: None,
                min_length: Some(2),
                max_length: Some(4),
                pattern: Some("^[a-z]+$".to_string()),
                format: None,
                default: None,
                enum_values: None,
                one_of: None,
            },
        )]),
    };
    let form = normalize_form("Name?".to_string(), schema).unwrap();

    let valid = QuestionRequestResponse::Submit {
        content: BTreeMap::from([("name".to_string(), QuestionValue::String("abc".to_string()))]),
    };
    assert!(validate_product_response(&form, &valid).is_ok());

    let unknown = QuestionRequestResponse::Submit {
        content: BTreeMap::from([
            ("name".to_string(), QuestionValue::String("abc".to_string())),
            ("extra".to_string(), QuestionValue::Boolean(true)),
        ]),
    };
    assert!(validate_product_response(&form, &unknown).is_err());

    let pattern = QuestionRequestResponse::Submit {
        content: BTreeMap::from([("name".to_string(), QuestionValue::String("A".to_string()))]),
    };
    assert!(validate_product_response(&form, &pattern).is_err());
}

#[test]
fn schema_budgets_reject_oversized_forms() {
    let properties = (0..33)
        .map(|index| {
            (
                format!("field-{index}"),
                PropertySchema::Boolean {
                    title: None,
                    description: None,
                    default: None,
                },
            )
        })
        .collect();
    let schema = ElicitationSchema {
        type_: ObjectType::Object,
        title: None,
        description: None,
        properties,
        required: Vec::new(),
    };

    assert!(normalize_form("Too many".to_string(), schema).is_err());
}
