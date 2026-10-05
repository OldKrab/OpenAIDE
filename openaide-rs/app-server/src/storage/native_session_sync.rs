//! Durable evidence used to tell external Native Session changes from the App Server's own.

use serde::{Deserialize, Serialize};

use super::TaskRecord;

/// A durable hint that a bound Native Session may contain external changes.
///
/// This is deliberately independent from process-local catalog freshness: the Agent's
/// `updatedAt` cannot prove what changed, so it authorizes a later explicit replay rather
/// than replacing an already editable attachment.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskNativeSessionReloadRequirement {
    pub observed_activity_at: String,
}

impl TaskRecord {
    /// Records only newer valid Agent activity so repeated catalog pages coalesce safely.
    pub(crate) fn mark_native_session_reload_required(
        &mut self,
        observed_activity_at: String,
    ) -> bool {
        let Some(observed_time) = crate::time::activity_millis(&observed_activity_at) else {
            return false;
        };
        if self
            .native_session_reload_requirement
            .as_ref()
            .and_then(|requirement| crate::time::activity_millis(&requirement.observed_activity_at))
            .is_some_and(|current_time| current_time >= observed_time)
        {
            return false;
        }
        self.native_session_reload_requirement = Some(TaskNativeSessionReloadRequirement {
            observed_activity_at,
        });
        true
    }

    /// A replay clears only the requirement it actually covered; a newer observation survives.
    pub(crate) fn clear_native_session_reload_requirement_through(
        &mut self,
        observed_activity_at: &str,
    ) -> bool {
        let Some(captured_time) = crate::time::activity_millis(observed_activity_at) else {
            return false;
        };
        if self
            .native_session_reload_requirement
            .as_ref()
            .and_then(|requirement| crate::time::activity_millis(&requirement.observed_activity_at))
            .is_some_and(|current_time| current_time <= captured_time)
        {
            self.native_session_reload_requirement = None;
            return true;
        }
        false
    }

    /// Records an App Server-initiated session operation; older values never win.
    pub(crate) fn record_own_native_session_activity(&mut self, at: &str) -> bool {
        let Some(time) = crate::time::activity_millis(at) else {
            return false;
        };
        if self
            .native_session_own_activity_at
            .as_deref()
            .and_then(crate::time::activity_millis)
            .is_some_and(|current| current >= time)
        {
            return false;
        }
        self.native_session_own_activity_at = Some(at.to_string());
        true
    }
}
