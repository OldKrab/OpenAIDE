use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::acp_host_capabilities::AcpSessionEventSinkMap;
use crate::agent::acp_live_prompt_projection::LivePromptProjection;
use crate::agent::acp_schema::{
    SessionNotification, SessionUpdate, SubagentSpawnedUpdate, SubagentState, SubagentStateUpdate,
};
use crate::agent::events::{
    AgentEvent, AgentNativeSubagentCapabilities, AgentNativeSubagentDetail,
    AgentNativeSubagentSpawned, AgentNativeSubagentState, AgentNativeSubagentStateUpdate,
    AgentPermissionOutcome, AgentPermissionRequest,
};
use crate::agent::{AgentEventSink, AgentSessionEventSink};
use crate::protocol::errors::RuntimeError;

#[derive(Clone)]
pub(super) struct AcpNativeSubagentRouter {
    agent_id: String,
    sinks: AcpSessionEventSinkMap,
    inner: Arc<Mutex<RouterState>>,
}

#[derive(Default)]
struct RouterState {
    negotiated: bool,
    children: HashMap<String, ChildRoute>,
}

#[derive(Clone)]
struct ChildRoute {
    parent_session_id: String,
    root_session_id: String,
    name: String,
    projection: LivePromptProjection,
    started_at: std::time::Instant,
}

pub(super) enum RoutedSessionNotification {
    Root(Box<SessionNotification>),
    Handled,
}

impl AcpNativeSubagentRouter {
    pub(super) fn new(agent_id: impl Into<String>, sinks: AcpSessionEventSinkMap) -> Self {
        Self {
            agent_id: agent_id.into(),
            sinks,
            inner: Arc::default(),
        }
    }

    pub(super) fn set_negotiated(&self, negotiated: bool) {
        let mut state = self.inner.lock().expect("ACP Subagent router poisoned");
        state.negotiated = negotiated;
        if !negotiated {
            state.children.clear();
        }
    }

    pub(super) fn route(
        &self,
        notification: SessionNotification,
    ) -> Result<RoutedSessionNotification, RuntimeError> {
        let outer_session_id = notification.session_id.to_string();
        let outer_meta = notification.meta;
        match notification.update {
            SessionUpdate::SubagentSpawned(spawned) => {
                self.spawn(outer_session_id, spawned)?;
                Ok(RoutedSessionNotification::Handled)
            }
            SessionUpdate::SubagentStateUpdate(update) => {
                self.state_changed(outer_session_id, update)?;
                Ok(RoutedSessionNotification::Handled)
            }
            update => {
                let projection = {
                    let state = self.inner.lock().expect("ACP Subagent router poisoned");
                    state
                        .children
                        .get(&outer_session_id)
                        .map(|route| route.projection.clone())
                };
                if let Some(projection) = projection {
                    projection.emit(update)?;
                    Ok(RoutedSessionNotification::Handled)
                } else {
                    Ok(RoutedSessionNotification::Root(Box::new(
                        SessionNotification::new(outer_session_id, update).meta(outer_meta),
                    )))
                }
            }
        }
    }

    pub(super) fn permission_attribution(
        &self,
        child_session_id: &str,
        tool_call: crate::agent::acp_schema::ToolCallUpdate,
    ) -> Result<Option<(String, String)>, RuntimeError> {
        let route = self
            .inner
            .lock()
            .expect("ACP Subagent router poisoned")
            .children
            .get(child_session_id)
            .cloned();
        let Some(route) = route else { return Ok(None) };
        route.projection.publish_permission_tool(tool_call)?;
        Ok(Some((route.root_session_id, route.name)))
    }

    pub(super) fn root_sink_for_child(
        &self,
        child_session_id: &str,
    ) -> Option<(String, Arc<dyn AgentSessionEventSink>)> {
        let (root_session_id, _) = self.child_attribution(child_session_id)?;
        let sink = self
            .sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .get(&root_session_id)
            .cloned()?;
        Some((root_session_id, sink))
    }

    pub(super) fn child_attribution(&self, child_session_id: &str) -> Option<(String, String)> {
        self.inner
            .lock()
            .expect("ACP Subagent router poisoned")
            .children
            .get(child_session_id)
            .map(|route| (route.root_session_id.clone(), route.name.clone()))
    }

    fn spawn(
        &self,
        parent_session_id: String,
        spawned: SubagentSpawnedUpdate,
    ) -> Result<(), RuntimeError> {
        let child_session_id = spawned.subagent_session_id.to_string();
        let (root_session_id, root_sink) = self.root_for_parent(&parent_session_id)?;
        let details = codex_details(spawned.meta.as_ref());
        let child_name = spawned.name.clone();
        let event = AgentNativeSubagentSpawned {
            parent_native_session_id: parent_session_id.clone(),
            native_session_id: child_session_id.clone(),
            name: spawned.name,
            delegated_task: spawned.task,
            capabilities: AgentNativeSubagentCapabilities {
                cancel: spawned.capabilities.cancel,
                close: spawned.capabilities.close,
            },
            details,
        };
        root_sink.subagent_spawned(event)?;
        let event_sink: Arc<dyn AgentEventSink> = Arc::new(SubagentEventSink {
            native_session_id: child_session_id.clone(),
            root_session_id: root_session_id.clone(),
            sinks: self.sinks.clone(),
        });
        let projection = LivePromptProjection::for_native_subagent(&self.agent_id, event_sink);
        let mut state = self.inner.lock().expect("ACP Subagent router poisoned");
        if !state.negotiated {
            return Err(RuntimeError::InvalidParams(
                "ACP Agent sent Subagent traffic without bilateral negotiation".to_string(),
            ));
        }
        crate::logging::info(
            "acp_subagent_session_started",
            serde_json::json!({
                "root_session_id": root_session_id,
                "parent_session_id": parent_session_id,
                "subagent_session_id": child_session_id,
            }),
        );
        state
            .children
            .entry(child_session_id)
            .or_insert(ChildRoute {
                parent_session_id,
                root_session_id,
                name: child_name,
                projection,
                started_at: std::time::Instant::now(),
            });
        Ok(())
    }

    fn state_changed(
        &self,
        parent_session_id: String,
        update: SubagentStateUpdate,
    ) -> Result<(), RuntimeError> {
        let child_session_id = update.subagent_session_id.to_string();
        let (root_session_id, expected_parent, started_at) = {
            let state = self.inner.lock().expect("ACP Subagent router poisoned");
            if !state.negotiated {
                return Err(RuntimeError::InvalidParams(
                    "ACP Agent sent Subagent traffic without bilateral negotiation".to_string(),
                ));
            }
            let route = state.children.get(&child_session_id).ok_or_else(|| {
                RuntimeError::InvalidParams("unknown ACP Subagent lifecycle".to_string())
            })?;
            (
                route.root_session_id.clone(),
                route.parent_session_id.clone(),
                route.started_at,
            )
        };
        if expected_parent != parent_session_id {
            return Err(RuntimeError::InvalidParams(
                "ACP Subagent terminal update has the wrong parent".to_string(),
            ));
        }
        let root_sink = self
            .sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .get(&root_session_id)
            .cloned()
            .ok_or_else(|| RuntimeError::NotReady("Task event sink is unavailable".to_string()))?;
        let state = match update.state {
            SubagentState::Completed => AgentNativeSubagentState::Completed,
            SubagentState::Failed => AgentNativeSubagentState::Failed,
            SubagentState::Cancelled => AgentNativeSubagentState::Cancelled,
            SubagentState::Disconnected => AgentNativeSubagentState::Disconnected,
            _ => AgentNativeSubagentState::Disconnected,
        };
        root_sink.subagent_state_changed(AgentNativeSubagentStateUpdate {
            parent_native_session_id: parent_session_id,
            native_session_id: child_session_id.clone(),
            state,
        })?;
        crate::logging::info(
            "acp_subagent_session_finished",
            serde_json::json!({
                "root_session_id": root_session_id,
                "subagent_session_id": child_session_id,
                "outcome": native_state_name(state),
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        Ok(())
    }

    fn root_for_parent(
        &self,
        parent_session_id: &str,
    ) -> Result<(String, Arc<dyn AgentSessionEventSink>), RuntimeError> {
        let negotiated = self
            .inner
            .lock()
            .expect("ACP Subagent router poisoned")
            .negotiated;
        if !negotiated {
            return Err(RuntimeError::InvalidParams(
                "ACP Agent sent Subagent traffic without bilateral negotiation".to_string(),
            ));
        }
        if let Some(sink) = self
            .sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .get(parent_session_id)
            .cloned()
        {
            return Ok((parent_session_id.to_string(), sink));
        }
        self.root_sink_for_child(parent_session_id)
            .ok_or_else(|| RuntimeError::InvalidParams("unknown ACP Subagent parent".to_string()))
    }
}

fn native_state_name(state: AgentNativeSubagentState) -> &'static str {
    match state {
        AgentNativeSubagentState::Completed => "completed",
        AgentNativeSubagentState::Failed => "failed",
        AgentNativeSubagentState::Cancelled => "cancelled",
        AgentNativeSubagentState::Disconnected => "disconnected",
    }
}

struct SubagentEventSink {
    native_session_id: String,
    root_session_id: String,
    sinks: AcpSessionEventSinkMap,
}

impl SubagentEventSink {
    fn root_sink(&self) -> Result<Arc<dyn AgentSessionEventSink>, RuntimeError> {
        self.sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .get(&self.root_session_id)
            .cloned()
            .ok_or_else(|| RuntimeError::NotReady("Task event sink is unavailable".to_string()))
    }
}

impl AgentEventSink for SubagentEventSink {
    fn emit(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        self.root_sink()?
            .subagent_session_update(&self.native_session_id, event)
    }

    fn request_permission(
        &self,
        request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        self.root_sink()?
            .request_subagent_permission(&self.native_session_id, request)
    }
}

/// Agent-specific metadata mapping is intentionally typed and allowlisted.
/// Unknown values are counted in diagnostics elsewhere, never sent to Frontend.
fn codex_details(
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Vec<AgentNativeSubagentDetail> {
    let Some(codex) = meta
        .and_then(|meta| meta.get("codex"))
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let subagent = codex.get("subagent").and_then(serde_json::Value::as_object);
    let collaboration = codex
        .get("collaboration")
        .and_then(serde_json::Value::as_object);
    [
        ("Path", subagent.and_then(|value| value.get("path"))),
        ("Role", collaboration.and_then(|value| value.get("role"))),
        ("Model", collaboration.and_then(|value| value.get("model"))),
        (
            "Reasoning",
            collaboration.and_then(|value| value.get("reasoningEffort")),
        ),
    ]
    .into_iter()
    .filter_map(|(label, value)| {
        value
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= 200)
            .map(|value| AgentNativeSubagentDetail {
                label: label.to_string(),
                value: value.to_string(),
            })
    })
    .collect()
}
