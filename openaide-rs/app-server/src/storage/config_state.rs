use crate::protocol::model::ConfigOptionCurrentValue;
use crate::storage::agent_config_preferences::AgentConfigPreference;
use serde::{Deserialize, Serialize};

/// App Server ordering state for one Task's Agent-owned configuration changes.
///
/// The sequence is monotonic across settled changes so a late Agent response can
/// never become authoritative again after a newer client mutation supersedes it.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskConfigMutationState {
    #[serde(default)]
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingTaskConfigChange>,
    /// Process-owned initialization of a new session from the Agent's preferences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences: Option<openaide_app_server_protocol::snapshot::AgentConfigPreferencesSnapshot>,
    /// Every value of the session's catalog as of the last user-initiated change it
    /// confirmed, including preferences applied to a new session. An Agent may restore a
    /// session with its defaults instead, so these are reapplied after it is restored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) chosen: Vec<AgentConfigPreference>,
}

impl TaskConfigMutationState {
    pub(super) fn is_empty(&self) -> bool {
        self.sequence == 0
            && self.pending.is_none()
            && self.preferences.is_none()
            && self.chosen.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PendingTaskConfigChange {
    pub sequence: u64,
    pub client_mutation_id: String,
    pub config_id: String,
    #[serde(deserialize_with = "deserialize_pending_config_value")]
    pub requested_value: ConfigOptionCurrentValue,
}

fn deserialize_pending_config_value<'de, D>(
    deserializer: D,
) -> Result<ConfigOptionCurrentValue, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if let Some(value) = value.as_str() {
        return Ok(ConfigOptionCurrentValue::id(value));
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}
