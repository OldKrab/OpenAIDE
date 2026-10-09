use openaide_app_server_protocol::snapshot::{
    AgentAccountLimitStatus, AgentAccountLimitWindowKind,
};
use serde_json::json;

use super::{
    is_account_limits_carrier, project_account_limits, project_account_limits_reading,
    AccountLimitsReading,
};
use crate::agent::events::{
    AgentAccountLimitUsage, AgentAccountLimitWindowId, AgentAccountLimitsChange,
};

fn window(kind: AgentAccountLimitWindowKind, model: Option<&str>) -> AgentAccountLimitWindowId {
    AgentAccountLimitWindowId {
        kind,
        model_label: model.map(str::to_string),
    }
}

fn meta(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value.as_object().expect("meta object").clone()
}

#[test]
fn absent_meta_and_unrelated_keys_project_nothing() {
    assert!(project_account_limits(None).changes.is_empty());
    let projection = project_account_limits(Some(&meta(json!({ "_claude/model": "opus" }))));
    assert!(projection.changes.is_empty());
    assert!(projection.ignored.is_empty());
}

#[test]
fn usage_reading_keeps_known_windows_and_drops_unknown_ones() {
    let projection = project_account_limits(Some(&meta(json!({
        "_claude/accountLimits": {
            "subscriptionType": "max",
            "windows": [
                { "type": "five_hour", "utilization": 62.6, "resetsAt": "2026-10-09T14:30:00Z" },
                { "type": "seven_day", "utilization": 140, "resetsAt": null },
                { "type": "seven_day_opus", "utilization": 9 },
                { "type": "seven_day_model", "model": "Fable 5", "utilization": 4 },
                { "type": "seven_day_model", "model": "<b>x</b>", "utilization": 4 },
                { "type": "seven_day_oauth_apps", "utilization": 1 },
                { "type": "five_hour" }
            ]
        }
    }))));

    assert_eq!(
        projection.changes,
        vec![AgentAccountLimitsChange::Usage {
            plan_label: Some("Max".to_string()),
            windows: vec![
                AgentAccountLimitUsage {
                    window: window(AgentAccountLimitWindowKind::FiveHour, None),
                    used_percent: 63,
                    resets_at_ms: Some(1_791_556_200_000),
                },
                AgentAccountLimitUsage {
                    window: window(AgentAccountLimitWindowKind::Weekly, None),
                    used_percent: 100,
                    resets_at_ms: None,
                },
                AgentAccountLimitUsage {
                    window: window(AgentAccountLimitWindowKind::WeeklyModel, Some("Opus")),
                    used_percent: 9,
                    resets_at_ms: None,
                },
                AgentAccountLimitUsage {
                    window: window(AgentAccountLimitWindowKind::WeeklyModel, Some("Fable 5")),
                    used_percent: 4,
                    resets_at_ms: None,
                },
            ],
        }]
    );
}

#[test]
fn rate_limit_verdict_maps_status_fraction_and_seconds() {
    let projection = project_account_limits(Some(&meta(json!({
        "_claude/rateLimit": {
            "status": "allowed_warning",
            "rateLimitType": "seven_day",
            "utilization": 0.91,
            "resetsAt": 1_791_556_200u64
        }
    }))));

    assert_eq!(
        projection.changes,
        vec![AgentAccountLimitsChange::Signal {
            window: window(AgentAccountLimitWindowKind::Weekly, None),
            status: AgentAccountLimitStatus::Warning,
            used_percent: Some(91),
            resets_at_ms: Some(1_791_556_200_000),
        }]
    );
}

#[test]
fn overage_and_windowless_verdicts_are_not_windows() {
    for value in [
        json!({ "status": "allowed" }),
        json!({ "status": "rejected", "rateLimitType": "overage" }),
    ] {
        let projection = project_account_limits(Some(&meta(json!({ "_claude/rateLimit": value }))));
        assert!(projection.changes.is_empty());
        assert!(projection.ignored.is_empty());
    }
}

#[test]
fn malformed_values_are_counted_not_guessed() {
    let projection = project_account_limits(Some(&meta(json!({
        "_claude/accountLimits": { "windows": "none" },
        "_claude/rateLimit": { "status": "throttled", "rateLimitType": "five_hour" }
    }))));
    assert!(projection.changes.is_empty());
    assert_eq!(projection.ignored, vec!["account_limits", "rate_limit"]);
}

#[test]
fn plan_label_rejects_free_form_text() {
    let projection = project_account_limits(Some(&meta(json!({
        "_claude/accountLimits": { "subscriptionType": "see https://example.test", "windows": [] }
    }))));
    assert_eq!(
        projection.changes,
        vec![AgentAccountLimitsChange::Usage {
            plan_label: None,
            windows: Vec::new(),
        }]
    );
}

#[test]
fn a_connection_reading_carries_the_same_windows_as_the_meta_key() {
    let reading = project_account_limits_reading(&json!({
        "accountLimits": {
            "subscriptionType": "pro",
            "windows": [{ "type": "five_hour", "utilization": 39, "resetsAt": null }],
        },
    }));
    assert_eq!(
        reading,
        AccountLimitsReading::Usage(AgentAccountLimitsChange::Usage {
            plan_label: Some("Pro".to_string()),
            windows: vec![AgentAccountLimitUsage {
                window: window(AgentAccountLimitWindowKind::FiveHour, None),
                used_percent: 39,
                resets_at_ms: None,
            }],
        })
    );
}

#[test]
fn a_connection_reading_tells_no_limits_apart_from_an_unusable_payload() {
    assert_eq!(
        project_account_limits_reading(&json!({ "accountLimits": null })),
        AccountLimitsReading::Unavailable
    );
    assert_eq!(
        project_account_limits_reading(&json!({})),
        AccountLimitsReading::Unavailable
    );
    assert_eq!(
        project_account_limits_reading(&json!({ "accountLimits": { "windows": "many" } })),
        AccountLimitsReading::Malformed
    );
}

#[test]
fn only_an_update_carrying_a_full_reading_is_a_limits_carrier() {
    assert!(!is_account_limits_carrier(None));
    // A verdict rides on the turn's own usage update, whose context usage is real.
    assert!(!is_account_limits_carrier(Some(&meta(
        json!({ "_claude/rateLimit": { "status": "allowed" } })
    ))));
    assert!(is_account_limits_carrier(Some(&meta(
        json!({ "_claude/accountLimits": { "windows": [] } })
    ))));
}
