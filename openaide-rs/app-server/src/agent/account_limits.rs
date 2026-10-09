//! Merges an Agent's account limit reports into the one value every Task of that Agent shows.
//!
//! Two reports feed it: a full usage reading (percentages and resets for every window) and a
//! per-request verdict (ok, warning, reached) for a single window. The verdict is the Agent's
//! own judgement and is kept across readings until the window visibly rolls over.

use openaide_app_server_protocol::snapshot::{
    AgentAccountLimitStatus, AgentAccountLimitWindow, AgentAccountLimitWindowKind,
    AgentAccountLimits,
};

use crate::agent::events::{
    AgentAccountLimitUsage, AgentAccountLimitWindowId, AgentAccountLimitsChange,
};

/// Returns the limits after `change`. `None` means the Agent meters no window for this account.
pub(crate) fn apply_account_limits_change(
    current: Option<&AgentAccountLimits>,
    change: AgentAccountLimitsChange,
) -> Option<AgentAccountLimits> {
    match change {
        AgentAccountLimitsChange::Usage {
            plan_label,
            windows,
        } => {
            let mut next = Vec::with_capacity(windows.len());
            for usage in windows {
                if find(&next, &usage.window).is_some() {
                    continue;
                }
                let previous = current.and_then(|limits| find(&limits.windows, &usage.window));
                next.push(window_after_usage(previous, usage));
            }
            sort_for_display(&mut next);
            (!next.is_empty()).then_some(AgentAccountLimits {
                plan_label: plan_label
                    .or_else(|| current.and_then(|limits| limits.plan_label.clone())),
                windows: next,
            })
        }
        AgentAccountLimitsChange::Signal {
            window,
            status,
            used_percent,
            resets_at_ms,
        } => {
            let mut limits = current.cloned().unwrap_or_default();
            let position = limits
                .windows
                .iter()
                .position(|candidate| same_window(candidate, &window));
            match position {
                Some(position) => {
                    let existing = &mut limits.windows[position];
                    existing.status = status;
                    existing.used_percent = used_percent
                        .unwrap_or_else(|| reached_floor(status, existing.used_percent));
                    existing.resets_at_ms = resets_at_ms.or(existing.resets_at_ms);
                }
                None => {
                    // A verdict without a percentage cannot draw a window unless it is spent.
                    let used_percent = used_percent
                        .or_else(|| (status == AgentAccountLimitStatus::Reached).then_some(100))?;
                    limits.windows.push(AgentAccountLimitWindow {
                        kind: window.kind,
                        model_label: window.model_label,
                        used_percent,
                        resets_at_ms,
                        status,
                    });
                    sort_for_display(&mut limits.windows);
                }
            }
            Some(limits)
        }
    }
}

fn window_after_usage(
    previous: Option<&AgentAccountLimitWindow>,
    usage: AgentAccountLimitUsage,
) -> AgentAccountLimitWindow {
    let status = if usage.used_percent >= 100 {
        AgentAccountLimitStatus::Reached
    } else {
        match previous {
            // A warning stands until usage drops, which only a window rollover does.
            Some(previous)
                if previous.status == AgentAccountLimitStatus::Warning
                    && usage.used_percent >= previous.used_percent =>
            {
                AgentAccountLimitStatus::Warning
            }
            _ => AgentAccountLimitStatus::Ok,
        }
    };
    AgentAccountLimitWindow {
        kind: usage.window.kind,
        model_label: usage.window.model_label,
        used_percent: usage.used_percent,
        resets_at_ms: usage
            .resets_at_ms
            .or_else(|| previous.and_then(|previous| previous.resets_at_ms)),
        status,
    }
}

fn reached_floor(status: AgentAccountLimitStatus, used_percent: u8) -> u8 {
    if status == AgentAccountLimitStatus::Reached {
        100
    } else {
        used_percent
    }
}

fn find<'a>(
    windows: &'a [AgentAccountLimitWindow],
    id: &AgentAccountLimitWindowId,
) -> Option<&'a AgentAccountLimitWindow> {
    windows.iter().find(|window| same_window(window, id))
}

fn same_window(window: &AgentAccountLimitWindow, id: &AgentAccountLimitWindowId) -> bool {
    window.kind == id.kind && window.model_label == id.model_label
}

fn sort_for_display(windows: &mut [AgentAccountLimitWindow]) {
    windows.sort_by(|left, right| {
        (kind_rank(left.kind), &left.model_label).cmp(&(kind_rank(right.kind), &right.model_label))
    });
}

fn kind_rank(kind: AgentAccountLimitWindowKind) -> u8 {
    match kind {
        AgentAccountLimitWindowKind::FiveHour => 0,
        AgentAccountLimitWindowKind::Weekly => 1,
        AgentAccountLimitWindowKind::WeeklyModel => 2,
    }
}

/// The most severe status across windows, for metadata-only diagnostics.
pub(crate) fn worst_status_name(limits: Option<&AgentAccountLimits>) -> &'static str {
    let Some(limits) = limits else {
        return "none";
    };
    let has = |status| limits.windows.iter().any(|window| window.status == status);
    if has(AgentAccountLimitStatus::Reached) {
        "reached"
    } else if has(AgentAccountLimitStatus::Warning) {
        "warning"
    } else {
        "ok"
    }
}

#[cfg(test)]
#[path = "account_limits_tests.rs"]
mod tests;
