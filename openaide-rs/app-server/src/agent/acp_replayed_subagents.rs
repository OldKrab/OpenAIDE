//! Turns the child traffic of a complete `session/load` replay into Subagent
//! facts for the Task. A load runs before its Task has an event sink, and an
//! adoption runs before the Task exists, so the replay cannot reach a Task
//! while it arrives. The live router runs here over a collecting sink instead,
//! which keeps one reading of the released form.

use std::sync::{Arc, Mutex};

use crate::agent::acp_host_capabilities::AcpSessionEventSinkMap;
use crate::agent::acp_native_subagents::AcpNativeSubagentRouter;
use crate::agent::acp_schema::SessionNotification;
use crate::agent::events::{
    AgentEvent, AgentNativeSubagentDetailsUpdate, AgentNativeSubagentIdentityUpdate,
    AgentNativeSubagentSpawned, AgentNativeSubagentStateUpdate, AgentReplayedSubagentEvent,
};
use crate::agent::AgentSessionEventSink;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentCommandsCatalog, ConfigOptionsCatalog};

/// `traffic` holds the root's Subagent updates and every child's session
/// updates, in arrival order. Traffic the live router would reject is dropped
/// the same way here, so one bad child update never fails the load.
pub(super) fn project_replayed_subagents(
    agent_id: &str,
    implementation_name: Option<&str>,
    root_session_id: &str,
    traffic: Vec<SessionNotification>,
) -> Vec<AgentReplayedSubagentEvent> {
    if traffic.is_empty() || !crate::agent::acp_host::native_subagents_enabled() {
        return Vec::new();
    }
    let collector = Arc::new(ReplayedSubagentCollector::default());
    let sinks: AcpSessionEventSinkMap = Arc::default();
    sinks
        .lock()
        .expect("ACP session event sink lock poisoned")
        .insert(root_session_id.to_string(), collector.clone());
    let router = AcpNativeSubagentRouter::new(agent_id, sinks);
    router.set_negotiated(true, implementation_name);
    let update_count = traffic.len();
    let mut rejected_count = 0_usize;
    for notification in traffic {
        // A root result is a delegation row, which the root replay already built.
        if let Err(error) = router.route(notification) {
            rejected_count += 1;
            crate::logging::warn(
                "acp_subagent_replay_update_rejected",
                serde_json::json!({
                    "root_session_id": root_session_id,
                    "error_kind": error.code(),
                }),
            );
        }
    }
    let events = std::mem::take(
        &mut *collector
            .events
            .lock()
            .expect("replayed Subagent events lock poisoned"),
    );
    crate::logging::info(
        "acp_subagent_replay_projected",
        serde_json::json!({
            "root_session_id": root_session_id,
            "update_count": update_count,
            "rejected_count": rejected_count,
            "event_count": events.len(),
        }),
    );
    events
}

#[derive(Default)]
struct ReplayedSubagentCollector {
    events: Mutex<Vec<AgentReplayedSubagentEvent>>,
}

impl ReplayedSubagentCollector {
    fn push(&self, event: AgentReplayedSubagentEvent) {
        self.events
            .lock()
            .expect("replayed Subagent events lock poisoned")
            .push(event);
    }
}

impl AgentSessionEventSink for ReplayedSubagentCollector {
    fn config_options_changed(&self, _catalog: ConfigOptionsCatalog) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn commands_changed(&self, _catalog: AgentCommandsCatalog) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn subagent_spawned(&self, event: AgentNativeSubagentSpawned) -> Result<(), RuntimeError> {
        self.push(AgentReplayedSubagentEvent::Spawned(event));
        Ok(())
    }

    fn subagent_session_update(
        &self,
        native_session_id: &str,
        event: AgentEvent,
    ) -> Result<(), RuntimeError> {
        self.push(AgentReplayedSubagentEvent::Update {
            native_session_id: native_session_id.to_string(),
            event,
        });
        Ok(())
    }

    fn subagent_state_changed(
        &self,
        event: AgentNativeSubagentStateUpdate,
    ) -> Result<(), RuntimeError> {
        self.push(AgentReplayedSubagentEvent::State(event));
        Ok(())
    }

    fn subagent_details_changed(
        &self,
        event: AgentNativeSubagentDetailsUpdate,
    ) -> Result<(), RuntimeError> {
        self.push(AgentReplayedSubagentEvent::Details(event));
        Ok(())
    }

    fn subagent_identity_changed(
        &self,
        event: AgentNativeSubagentIdentityUpdate,
    ) -> Result<(), RuntimeError> {
        self.push(AgentReplayedSubagentEvent::Identity(event));
        Ok(())
    }
}

#[cfg(test)]
#[path = "acp_replayed_subagents_tests.rs"]
mod tests;
