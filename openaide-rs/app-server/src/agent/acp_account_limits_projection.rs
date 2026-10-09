//! Reads the Claude adapter's account limit values from a `usage_update`'s `_meta`.
//!
//! This is the one typed exception to "Agent-private `_meta` is never interpreted as usage": the
//! two keys below are parsed field by field, and anything unrecognized is dropped rather than
//! guessed at. Standard ACP has no account limit shape yet.

use openaide_app_server_protocol::snapshot::{
    AgentAccountLimitStatus, AgentAccountLimitWindowKind,
};
use serde_json::Value;

use crate::agent::events::{
    AgentAccountLimitUsage, AgentAccountLimitWindowId, AgentAccountLimitsChange,
};

/// Full reading of the account's windows, fetched by the adapter after a turn.
const ACCOUNT_LIMITS_KEY: &str = "_claude/accountLimits";
/// Claude's verdict for the window a request was checked against.
const RATE_LIMIT_KEY: &str = "_claude/rateLimit";

/// What one `usage_update` said about account limits, plus how many recognized keys were
/// present but unusable, so the caller can log a contract drift without logging the payload.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct AccountLimitsProjection {
    pub(super) changes: Vec<AgentAccountLimitsChange>,
    pub(super) ignored: Vec<&'static str>,
}

pub(super) fn project_account_limits(
    meta: Option<&serde_json::Map<String, Value>>,
) -> AccountLimitsProjection {
    let mut projection = AccountLimitsProjection::default();
    let Some(meta) = meta else {
        return projection;
    };
    if let Some(value) = meta.get(ACCOUNT_LIMITS_KEY) {
        match usage_change(value) {
            Some(change) => projection.changes.push(change),
            None => projection.ignored.push("account_limits"),
        }
    }
    if let Some(value) = meta.get(RATE_LIMIT_KEY) {
        match signal_change(value) {
            SignalProjection::Change(change) => projection.changes.push(change),
            // Overage and pay-as-you-go verdicts are not subscription windows.
            SignalProjection::NotAWindow => {}
            SignalProjection::Malformed => projection.ignored.push("rate_limit"),
        }
    }
    projection
}

fn usage_change(value: &Value) -> Option<AgentAccountLimitsChange> {
    let windows = value
        .get("windows")?
        .as_array()?
        .iter()
        .filter_map(|window| {
            Some(AgentAccountLimitUsage {
                window: usage_window_id(window)?,
                used_percent: percent(window.get("utilization")?.as_f64()?),
                resets_at_ms: window
                    .get("resetsAt")
                    .and_then(Value::as_str)
                    .and_then(crate::time::activity_millis)
                    .and_then(|millis| u64::try_from(millis).ok()),
            })
        })
        .collect();
    Some(AgentAccountLimitsChange::Usage {
        plan_label: value
            .get("subscriptionType")
            .and_then(Value::as_str)
            .and_then(plan_label),
        windows,
    })
}

enum SignalProjection {
    Change(AgentAccountLimitsChange),
    NotAWindow,
    Malformed,
}

fn signal_change(value: &Value) -> SignalProjection {
    let status = match value.get("status").and_then(Value::as_str) {
        Some("allowed") => AgentAccountLimitStatus::Ok,
        Some("allowed_warning") => AgentAccountLimitStatus::Warning,
        Some("rejected") => AgentAccountLimitStatus::Reached,
        _ => return SignalProjection::Malformed,
    };
    let Some(window) = value.get("rateLimitType").and_then(Value::as_str) else {
        // An `allowed` verdict often names no window; there is nothing to attach it to.
        return SignalProjection::NotAWindow;
    };
    let Some(window) = window_id(window) else {
        return SignalProjection::NotAWindow;
    };
    SignalProjection::Change(AgentAccountLimitsChange::Signal {
        window,
        status,
        // The SDK reports this utilization as a 0..1 fraction, unlike the 0..100 usage reading.
        used_percent: value
            .get("utilization")
            .and_then(Value::as_f64)
            .map(|fraction| percent(fraction * 100.0)),
        // Unix seconds on the wire.
        resets_at_ms: value
            .get("resetsAt")
            .and_then(Value::as_u64)
            .map(|seconds| seconds.saturating_mul(1000)),
    })
}

/// A reading may also meter a model the adapter names itself, beyond the fixed window types.
fn usage_window_id(window: &Value) -> Option<AgentAccountLimitWindowId> {
    let wire = window.get("type")?.as_str()?;
    if wire != "seven_day_model" {
        return window_id(wire);
    }
    let model = window.get("model")?.as_str()?.trim();
    let valid = !model.is_empty()
        && model.len() <= 32
        && model
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, ' ' | '.' | '-'));
    valid.then(|| AgentAccountLimitWindowId {
        kind: AgentAccountLimitWindowKind::WeeklyModel,
        model_label: Some(model.to_string()),
    })
}

fn window_id(wire: &str) -> Option<AgentAccountLimitWindowId> {
    let (kind, model_label) = match wire {
        "five_hour" => (AgentAccountLimitWindowKind::FiveHour, None),
        "seven_day" => (AgentAccountLimitWindowKind::Weekly, None),
        "seven_day_opus" => (AgentAccountLimitWindowKind::WeeklyModel, Some("Opus")),
        "seven_day_sonnet" => (AgentAccountLimitWindowKind::WeeklyModel, Some("Sonnet")),
        _ => return None,
    };
    Some(AgentAccountLimitWindowId {
        kind,
        model_label: model_label.map(str::to_string),
    })
}

fn percent(value: f64) -> u8 {
    if value.is_finite() {
        value.round().clamp(0.0, 100.0) as u8
    } else {
        0
    }
}

/// Plan ids are short lowercase words ("max", "pro", "team"); anything else is not shown.
fn plan_label(wire: &str) -> Option<String> {
    let valid = !wire.is_empty()
        && wire.len() <= 24
        && wire
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if !valid {
        return None;
    }
    let spaced = wire.replace('_', " ");
    let mut chars = spaced.chars();
    let first = chars.next()?.to_ascii_uppercase();
    Some(format!("{first}{}", chars.as_str()))
}

#[cfg(test)]
#[path = "acp_account_limits_projection_tests.rs"]
mod tests;
