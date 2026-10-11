//! The `authStatus` ACP extension: an Agent process pushes which credentials it holds.
//!
//! Push only and connection-scoped. OpenAIDE reads one fact from it, whether the process is
//! signed in at all, so a signed-out Agent asks for sign-in before a prompt is refused and a
//! login made in another terminal ends that request. The identity itself is not stored.

use serde_json::{json, Value};

use crate::agent::status_cache::AgentReportRecorder;
use crate::logging;

pub(super) const AUTH_STATUS_UPDATE_METHOD: &str = "_auth/status_update";

/// `Some(false)` only for the known signed-out state. Kinds added later are credentials of
/// some sort, so they read as signed in; a payload without a kind says nothing.
pub(super) fn reported_signed_in(params: &Value) -> Option<bool> {
    let kind = params.get("authStatus")?.get("kind")?.as_str()?;
    Some(kind != "none")
}

pub(super) fn record_pushed_auth_status(recorder: Option<&AgentReportRecorder>, params: &Value) {
    match reported_signed_in(params) {
        Some(signed_in) => {
            if let Some(recorder) = recorder {
                recorder.record_signed_in(signed_in);
            }
        }
        None => logging::warn(
            "acp_auth_status_ignored",
            json!({ "source": "notification", "reason": "kind_missing" }),
        ),
    }
}

#[cfg(test)]
#[path = "acp_auth_status_tests.rs"]
mod tests;
