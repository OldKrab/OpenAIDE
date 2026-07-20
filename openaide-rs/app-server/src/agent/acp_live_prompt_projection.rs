use std::sync::Arc;

#[cfg(test)]
use crate::agent::acp_schema::ToolCallUpdate;
use crate::agent::acp_schema::{
    PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionUpdate, ToolCall,
};
use serde_json::json;

use crate::agent::acp_config_projection::normalize_config_options;
use crate::agent::acp_content_projection::project_content_block;
#[cfg(test)]
use crate::agent::acp_tool_call_projection::{merge_tool_call_update, remember_tool_call};
use crate::agent::acp_tool_call_projection::{
    merge_tool_call_update_with_changes, merge_tool_call_update_with_status_change,
    remember_tool_call_with_status_change, tool_status_name, ToolCallState,
};
use crate::agent::acp_update_projection::normalize_available_commands;
use crate::agent::events::{
    AgentEvent, AgentPermissionOption, AgentPermissionOptionKind, AgentPermissionOutcome,
    AgentPermissionRequest, AgentToolCallRef,
};
use crate::agent::tool_details::{tool_call_event, tool_kind_name};
use crate::agent::{AgentEventSink, AgentSessionEventSink, TurnCancellation};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::AgentMessageRole;

#[derive(Clone)]
pub(super) struct LivePromptProjection {
    agent_id: String,
    sink: Arc<dyn AgentEventSink>,
    tool_calls: ToolCallState,
    cancellation: TurnCancellation,
}

impl LivePromptProjection {
    #[cfg(test)]
    pub(super) fn new(
        agent_id: impl Into<String>,
        sink: Arc<dyn AgentEventSink>,
        cancellation: TurnCancellation,
    ) -> Self {
        Self::for_prompt(agent_id, sink, cancellation, None)
    }

    /// Keeps permission tool attribution on the same Native Session tool state
    /// that receives permanent session updates.
    pub(super) fn for_prompt(
        agent_id: impl Into<String>,
        sink: Arc<dyn AgentEventSink>,
        cancellation: TurnCancellation,
        session_projection: Option<&Self>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            sink,
            tool_calls: session_projection
                .map(|projection| projection.tool_calls.clone())
                .unwrap_or_default(),
            cancellation,
        }
    }

    pub(super) fn cancellation(&self) -> TurnCancellation {
        self.cancellation.clone()
    }

    /// Creates the projection that survives individual session/prompt requests.
    pub(super) fn for_session(
        agent_id: impl Into<String>,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Self {
        Self::for_prompt(
            agent_id,
            Arc::new(SessionUpdateEventSink { sink }),
            TurnCancellation::new(),
            None,
        )
    }

    #[cfg(test)]
    pub(super) fn remember_tool_call(&self, tool_call: ToolCall) {
        remember_tool_call(&self.tool_calls, tool_call);
    }

    #[cfg(test)]
    pub(super) fn merge_tool_call_update(&self, update: ToolCallUpdate) -> ToolCall {
        merge_tool_call_update(&self.tool_calls, update)
    }

    pub(super) async fn permission_response(
        self,
        request: RequestPermissionRequest,
    ) -> Result<RequestPermissionResponse, RuntimeError> {
        let (tool_call, status_changed) =
            merge_tool_call_update_with_status_change(&self.tool_calls, request.tool_call.clone());
        // ACP permission requests carry the authoritative tool-call update. Publish it
        // before waiting so Chat shows the activity beside the transient request even
        // when the Agent did not send a separate tool-call notification first.
        self.publish_tool_call(&tool_call, status_changed)?;
        let permission = permission_request_from_acp(request, &tool_call);
        logging::info(
            "acp_permission_bridge_wait_start",
            json!({
                "agent_id": self.agent_id.as_str(),
                "agent_request_id": permission.request_id.as_str(),
                "tool_call_id": permission.tool_call.tool_call_id.as_str(),
                "tool_kind": permission.tool_call.kind.as_deref(),
                "option_count": permission.options.len(),
            }),
        );
        let agent_id = self.agent_id.clone();
        let agent_request_id = permission.request_id.clone();
        let tool_call_id = permission.tool_call.tool_call_id.clone();
        let sink = self.sink.clone();
        let selected = tokio::task::spawn_blocking(move || sink.request_permission(permission))
            .await
            .map_err(|error| RuntimeError::Internal(error.to_string()))??;
        logging::info(
            "acp_permission_bridge_wait_end",
            json!({
                "agent_id": agent_id,
                "agent_request_id": agent_request_id,
                "tool_call_id": tool_call_id,
                "outcome": agent_permission_outcome_name(&selected),
            }),
        );
        Ok(match selected {
            AgentPermissionOutcome::Selected { option_id } => RequestPermissionResponse::new(
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
            ),
            AgentPermissionOutcome::Cancelled => {
                RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
            }
        })
    }

    pub(super) fn emit(&self, update: SessionUpdate) -> Result<(), RuntimeError> {
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                self.sink.emit(AgentEvent::MessageChunk {
                    role: AgentMessageRole::Agent,
                    part: project_content_block(chunk.content, AgentMessageRole::Agent),
                    source_message_id: chunk.message_id.map(|id| id.to_string()),
                })?
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                self.sink.emit(AgentEvent::MessageChunk {
                    role: AgentMessageRole::Thought,
                    part: project_content_block(chunk.content, AgentMessageRole::Thought),
                    source_message_id: chunk.message_id.map(|id| id.to_string()),
                })?
            }
            SessionUpdate::ToolCall(tool_call) => {
                let status_changed =
                    remember_tool_call_with_status_change(&self.tool_calls, tool_call.clone());
                self.publish_tool_call(&tool_call, status_changed)?;
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let (tool_call, status_changed, projection_changed) =
                    merge_tool_call_update_with_changes(&self.tool_calls, update);
                if projection_changed {
                    self.publish_tool_call(&tool_call, status_changed)?;
                }
            }
            SessionUpdate::ConfigOptionUpdate(update) => {
                self.sink
                    .emit(AgentEvent::ConfigOptionsChanged(normalize_config_options(
                        &self.agent_id,
                        update.config_options,
                    )))?;
            }
            SessionUpdate::AvailableCommandsUpdate(update) => {
                self.sink
                    .emit(AgentEvent::CommandsChanged(normalize_available_commands(
                        update,
                    )))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn publish_tool_call(
        &self,
        tool_call: &ToolCall,
        status_changed: bool,
    ) -> Result<(), RuntimeError> {
        let AgentEvent::ToolCall(event) = tool_call_event(tool_call) else {
            unreachable!("tool_call_event always returns a tool event");
        };
        if status_changed {
            logging::info(
                "acp_tool_call_status_projected",
                json!({
                    "agent_id": self.agent_id.as_str(),
                    "tool_call_id": tool_call.tool_call_id.to_string(),
                    "tool_kind": tool_kind_name(tool_call.kind),
                    "tool_status": tool_status_name(&tool_call.status),
                }),
            );
        }
        self.sink.emit(AgentEvent::ToolCall(event))
    }
}

struct SessionUpdateEventSink {
    sink: Arc<dyn AgentSessionEventSink>,
}

impl AgentEventSink for SessionUpdateEventSink {
    fn emit(&self, event: AgentEvent) -> Result<(), RuntimeError> {
        // Catalogs and metadata retain their dedicated typed session callbacks.
        if matches!(
            event,
            AgentEvent::ConfigOptionsChanged(_) | AgentEvent::CommandsChanged(_)
        ) {
            return Ok(());
        }
        self.sink.session_update(event)
    }

    fn request_permission(
        &self,
        _request: AgentPermissionRequest,
    ) -> Result<AgentPermissionOutcome, RuntimeError> {
        Ok(AgentPermissionOutcome::Cancelled)
    }
}

fn agent_permission_outcome_name(outcome: &AgentPermissionOutcome) -> &'static str {
    match outcome {
        AgentPermissionOutcome::Selected { .. } => "selected",
        AgentPermissionOutcome::Cancelled => "cancelled",
    }
}

fn permission_request_from_acp(
    request: RequestPermissionRequest,
    tool_call: &ToolCall,
) -> AgentPermissionRequest {
    let tool_call_id = tool_call.tool_call_id.to_string();
    let title = tool_call.title.clone();
    AgentPermissionRequest {
        request_id: format!("acp_perm_{}", uuid::Uuid::new_v4()),
        title: title.clone(),
        description: None,
        scope: None,
        risk: None,
        tool_call: AgentToolCallRef {
            tool_call_id,
            title,
            kind: Some(tool_kind_name(tool_call.kind)),
        },
        options: request
            .options
            .into_iter()
            .map(|option| AgentPermissionOption {
                option_id: option.option_id.to_string(),
                name: option.name,
                kind: permission_kind(option.kind),
            })
            .collect(),
    }
}

fn permission_kind(kind: PermissionOptionKind) -> AgentPermissionOptionKind {
    match kind {
        PermissionOptionKind::AllowOnce => AgentPermissionOptionKind::AllowOnce,
        PermissionOptionKind::AllowAlways => AgentPermissionOptionKind::AllowAlways,
        PermissionOptionKind::RejectOnce => AgentPermissionOptionKind::RejectOnce,
        PermissionOptionKind::RejectAlways => AgentPermissionOptionKind::RejectAlways,
        _ => AgentPermissionOptionKind::RejectOnce,
    }
}
