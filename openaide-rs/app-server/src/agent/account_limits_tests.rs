use openaide_app_server_protocol::snapshot::{
    AgentAccountLimitStatus as Status, AgentAccountLimitWindowKind as Kind, AgentAccountLimits,
};

use super::apply_account_limits_change;
use crate::agent::events::{
    AgentAccountLimitUsage, AgentAccountLimitWindowId, AgentAccountLimitsChange,
};

fn id(kind: Kind, model: Option<&str>) -> AgentAccountLimitWindowId {
    AgentAccountLimitWindowId {
        kind,
        model_label: model.map(str::to_string),
    }
}

fn usage(windows: &[(Kind, Option<&str>, u8, Option<u64>)]) -> AgentAccountLimitsChange {
    AgentAccountLimitsChange::Usage {
        plan_label: Some("Max".to_string()),
        windows: windows
            .iter()
            .map(
                |(kind, model, used_percent, resets_at_ms)| AgentAccountLimitUsage {
                    window: id(*kind, *model),
                    used_percent: *used_percent,
                    resets_at_ms: *resets_at_ms,
                },
            )
            .collect(),
    }
}

fn signal(kind: Kind, status: Status, used_percent: Option<u8>) -> AgentAccountLimitsChange {
    AgentAccountLimitsChange::Signal {
        window: id(kind, None),
        status,
        used_percent,
        resets_at_ms: None,
    }
}

fn statuses(limits: &AgentAccountLimits) -> Vec<(Kind, u8, Status)> {
    limits
        .windows
        .iter()
        .map(|window| (window.kind, window.used_percent, window.status))
        .collect()
}

#[test]
fn usage_reading_orders_windows_for_display() {
    let limits = apply_account_limits_change(
        None,
        usage(&[
            (Kind::WeeklyModel, Some("Sonnet"), 3, None),
            (Kind::Weekly, None, 40, None),
            (Kind::WeeklyModel, Some("Opus"), 9, None),
            (Kind::FiveHour, None, 63, Some(5)),
        ]),
    )
    .expect("limits");

    assert_eq!(limits.plan_label.as_deref(), Some("Max"));
    let order: Vec<_> = limits
        .windows
        .iter()
        .map(|window| (window.kind, window.model_label.as_deref()))
        .collect();
    assert_eq!(
        order,
        vec![
            (Kind::FiveHour, None),
            (Kind::Weekly, None),
            (Kind::WeeklyModel, Some("Opus")),
            (Kind::WeeklyModel, Some("Sonnet")),
        ]
    );
}

#[test]
fn warning_survives_readings_until_the_window_rolls_over() {
    let limits = apply_account_limits_change(None, usage(&[(Kind::FiveHour, None, 80, Some(5))]));
    let limits = apply_account_limits_change(
        limits.as_ref(),
        signal(Kind::FiveHour, Status::Warning, None),
    );
    assert_eq!(
        statuses(limits.as_ref().expect("limits")),
        vec![(Kind::FiveHour, 80, Status::Warning)]
    );

    let limits =
        apply_account_limits_change(limits.as_ref(), usage(&[(Kind::FiveHour, None, 86, None)]));
    let limits = limits.expect("limits");
    assert_eq!(
        statuses(&limits),
        vec![(Kind::FiveHour, 86, Status::Warning)]
    );
    assert_eq!(limits.windows[0].resets_at_ms, Some(5));

    let limits =
        apply_account_limits_change(Some(&limits), usage(&[(Kind::FiveHour, None, 2, Some(9))]));
    assert_eq!(
        statuses(limits.as_ref().expect("limits")),
        vec![(Kind::FiveHour, 2, Status::Ok)]
    );
}

#[test]
fn reached_verdict_fills_the_window_and_a_later_reading_releases_it() {
    let limits = apply_account_limits_change(None, signal(Kind::Weekly, Status::Reached, None));
    let limits = limits.expect("limits");
    assert_eq!(
        statuses(&limits),
        vec![(Kind::Weekly, 100, Status::Reached)]
    );

    let limits =
        apply_account_limits_change(Some(&limits), usage(&[(Kind::Weekly, None, 100, None)]));
    let limits = limits.expect("limits");
    assert_eq!(
        statuses(&limits),
        vec![(Kind::Weekly, 100, Status::Reached)]
    );

    let limits =
        apply_account_limits_change(Some(&limits), usage(&[(Kind::Weekly, None, 1, None)]));
    assert_eq!(
        statuses(limits.as_ref().expect("limits")),
        vec![(Kind::Weekly, 1, Status::Ok)]
    );
}

#[test]
fn verdict_without_a_percentage_cannot_create_a_window() {
    assert_eq!(
        apply_account_limits_change(None, signal(Kind::FiveHour, Status::Warning, None)),
        None
    );
    let limits =
        apply_account_limits_change(None, signal(Kind::FiveHour, Status::Warning, Some(91)));
    assert_eq!(
        statuses(limits.as_ref().expect("limits")),
        vec![(Kind::FiveHour, 91, Status::Warning)]
    );
}

#[test]
fn empty_reading_clears_limits() {
    let limits = apply_account_limits_change(None, usage(&[(Kind::FiveHour, None, 10, None)]));
    assert_eq!(
        apply_account_limits_change(limits.as_ref(), usage(&[])),
        None
    );
}
