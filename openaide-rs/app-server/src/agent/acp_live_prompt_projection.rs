use std::sync::Arc;
use std::time::Instant;

use crate::agent::acp_schema::{
    PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionUpdate, ToolCall, ToolCallUpdate,
};
use serde_json::json;

use crate::agent::acp_codex_subagent::{
    project_codex_collaboration, CodexSubagentProjection, CodexSubagentState,
};
use crate::agent::acp_config_projection::normalize_config_options;
use crate::agent::acp_content_projection::project_content_block;
use crate::agent::acp_terminal_output_adapter::terminal_append;
#[cfg(test)]
use crate::agent::acp_tool_call_projection::{merge_tool_call_update, remember_tool_call};
use crate::agent::acp_tool_call_projection::{
    merge_tool_call_update_with_changes, merge_tool_call_update_with_status_change,
    remember_tool_call_with_status_change, tool_status_name, ToolCallState,
};
use crate::agent::acp_update_projection::normalize_available_commands;
use crate::agent::events::{
    AgentContextUsage, AgentEvent, AgentPermissionOption, AgentPermissionOptionKind,
    AgentPermissionOutcome, AgentPermissionRequest, AgentToolCallRef, AgentToolUpdate,
    AgentUsageCost,
};
use crate::agent::tool_details::{tool_call_event, tool_kind_name};
use crate::agent::{AgentEventSink, AgentSessionEventSink, TurnCancellation};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::AgentMessageRole;
use crate::protocol::model::{AgentPlan, AgentPlanEntry, AgentPlanPriority, AgentPlanStatus};

/// Asks the Native Session worker to project queued `session/update`s before a
/// permission publishes its Tool, so already-received anonymous text is not split.
pub(super) type PrecedingUpdateDrain =
    tokio::sync::mpsc::UnboundedSender<tokio::sync::oneshot::Sender<Result<(), RuntimeError>>>;

#[derive(Clone)]
pub(super) struct LivePromptProjection {
    agent_id: String,
    sink: Arc<dyn AgentEventSink>,
    tool_calls: ToolCallState,
    codex_subagents: CodexSubagentState,
    cancellation: TurnCancellation,
    preceding_update_drain: Option<PrecedingUpdateDrain>,
    project_user_messages: bool,
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
            codex_subagents: session_projection
                .map(|projection| projection.codex_subagents.clone())
                .unwrap_or_default(),
            cancellation,
            preceding_update_drain: None,
            project_user_messages: false,
        }
    }

    pub(super) fn with_preceding_update_drain(mut self, drain: PrecedingUpdateDrain) -> Self {
        self.preceding_update_drain = Some(drain);
        self
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

    pub(super) fn for_native_subagent(
        agent_id: impl Into<String>,
        sink: Arc<dyn AgentEventSink>,
    ) -> Self {
        let mut projection = Self::for_prompt(agent_id, sink, TurnCancellation::new(), None);
        // Codex currently projects an encrypted causal-root placeholder as a child User
        // message. It is not the delegated prompt, so presenting it invents transcript data.
        projection.project_user_messages = projection.agent_id != "codex";
        projection
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
        self.permission_response_inner(request, true, None).await
    }

    /// Child-session permission requests project their authoritative Tool call into
    /// child history first, then use the root prompt only for the task-wide request.
    pub(super) async fn permission_response_without_tool_projection(
        self,
        request: RequestPermissionRequest,
        subagent_native_session_id: String,
    ) -> Result<RequestPermissionResponse, RuntimeError> {
        self.permission_response_inner(request, false, Some(subagent_native_session_id))
            .await
    }

    pub(super) fn publish_permission_tool(
        &self,
        update: ToolCallUpdate,
    ) -> Result<(), RuntimeError> {
        let (tool_call, status_changed) =
            merge_tool_call_update_with_status_change(&self.tool_calls, update);
        self.publish_tool_call(&tool_call, status_changed)
    }

    async fn permission_response_inner(
        self,
        request: RequestPermissionRequest,
        publish_tool: bool,
        subagent_native_session_id: Option<String>,
    ) -> Result<RequestPermissionResponse, RuntimeError> {
        self.drain_preceding_session_updates().await?;
        let (tool_call, status_changed) =
            merge_tool_call_update_with_status_change(&self.tool_calls, request.tool_call.clone());
        // ACP permission requests carry the authoritative tool-call update. Publish it
        // before waiting so Chat shows the activity beside the transient request even
        // when the Agent did not send a separate tool-call notification first.
        if publish_tool {
            self.publish_tool_call(&tool_call, status_changed)?;
        }
        let permission =
            permission_request_from_acp(request, &tool_call, subagent_native_session_id);
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

    async fn drain_preceding_session_updates(&self) -> Result<(), RuntimeError> {
        let Some(drain) = self.preceding_update_drain.as_ref() else {
            return Ok(());
        };
        let started = Instant::now();
        logging::info(
            "acp_permission_preceding_updates_drain_start",
            json!({
                "agent_id": self.agent_id.as_str(),
            }),
        );
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if drain.send(reply_tx).is_err() {
            logging::info(
                "acp_permission_preceding_updates_drain_end",
                json!({
                    "agent_id": self.agent_id.as_str(),
                    "outcome": "closed",
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            return Ok(());
        }
        let result = reply_rx.await.map_err(|_| {
            RuntimeError::NotReady("ACP preceding-update drain dropped".to_string())
        })?;
        logging::info(
            "acp_permission_preceding_updates_drain_end",
            json!({
                "agent_id": self.agent_id.as_str(),
                "outcome": if result.is_ok() { "ok" } else { "error" },
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        result
    }

    pub(super) fn emit(&self, update: SessionUpdate) -> Result<(), RuntimeError> {
        match update {
            SessionUpdate::UserMessageChunk(chunk) if self.project_user_messages => {
                if let crate::agent::acp_schema::ContentBlock::Text(text) = chunk.content {
                    self.sink.emit(AgentEvent::UserMessageChunk {
                        text: text.text,
                        source_message_id: chunk.message_id.map(|id| id.to_string()),
                    })?;
                }
            }
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
                if self.agent_id == "codex" {
                    match self.codex_subagents.project_tool_call(&tool_call) {
                        CodexSubagentProjection::Event(subagent) => {
                            self.sink.emit(AgentEvent::Subagent(subagent))?
                        }
                        CodexSubagentProjection::Suppress => {}
                        CodexSubagentProjection::GenericTool => {
                            self.publish_tool_call(&tool_call, status_changed)?
                        }
                    }
                } else {
                    self.publish_tool_call(&tool_call, status_changed)?;
                }
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let subagent_projection = (self.agent_id == "codex")
                    .then(|| self.codex_subagents.project_tool_update(&update));
                let terminal_append = terminal_append(&self.agent_id, &update);
                let (tool_call, status_changed, projection_changed) =
                    merge_tool_call_update_with_changes(&self.tool_calls, update);
                match subagent_projection {
                    Some(CodexSubagentProjection::Event(subagent)) => {
                        self.sink.emit(AgentEvent::Subagent(subagent))?;
                        return Ok(());
                    }
                    Some(CodexSubagentProjection::Suppress) => return Ok(()),
                    _ => {}
                }
                if let Some(terminal_append) = terminal_append {
                    let summary = projection_changed
                        .then(|| self.project_tool_call(&tool_call, status_changed));
                    self.sink.emit(AgentEvent::ToolUpdate(AgentToolUpdate {
                        summary,
                        terminal_appends: vec![terminal_append],
                    }))?;
                } else if projection_changed {
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
            SessionUpdate::UsageUpdate(update) => {
                self.sink.emit(AgentEvent::ContextUsage(AgentContextUsage {
                    used_tokens: update.used,
                    capacity_tokens: update.size,
                    cost: update.cost.map(|cost| AgentUsageCost {
                        amount: cost.amount.to_string(),
                        currency: cost.currency,
                    }),
                }))?;
            }
            SessionUpdate::Plan(plan) => {
                let Some(entries) = plan
                    .entries
                    .into_iter()
                    .map(project_plan_entry)
                    .collect::<Option<Vec<_>>>()
                else {
                    logging::warn(
                        "acp_plan_update_ignored",
                        json!({
                            "agent_id": self.agent_id.as_str(),
                            "reason": "unsupported plan entry enum value",
                        }),
                    );
                    return Ok(());
                };
                self.sink.emit(AgentEvent::Plan(AgentPlan { entries }))?;
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
        self.sink.emit(AgentEvent::ToolCall(
            self.project_tool_call(tool_call, status_changed),
        ))
    }

    fn project_tool_call(
        &self,
        tool_call: &ToolCall,
        status_changed: bool,
    ) -> crate::agent::events::AgentToolCall {
        let event = if self.agent_id == "codex" {
            project_codex_collaboration(tool_call)
        } else {
            None
        }
        .unwrap_or_else(|| {
            let AgentEvent::ToolCall(event) = tool_call_event(tool_call) else {
                unreachable!("tool_call_event always returns a tool event");
            };
            event
        });
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
        event
    }
}

pub(super) fn project_plan_entry(
    entry: crate::agent::acp_schema::PlanEntry,
) -> Option<AgentPlanEntry> {
    let priority = match entry.priority {
        crate::agent::acp_schema::PlanEntryPriority::High => AgentPlanPriority::High,
        crate::agent::acp_schema::PlanEntryPriority::Medium => AgentPlanPriority::Medium,
        crate::agent::acp_schema::PlanEntryPriority::Low => AgentPlanPriority::Low,
        _ => return None,
    };
    let status = match entry.status {
        crate::agent::acp_schema::PlanEntryStatus::Pending => AgentPlanStatus::Pending,
        crate::agent::acp_schema::PlanEntryStatus::InProgress => AgentPlanStatus::InProgress,
        crate::agent::acp_schema::PlanEntryStatus::Completed => AgentPlanStatus::Completed,
        _ => return None,
    };
    Some(AgentPlanEntry {
        content: entry.content,
        priority,
        status,
    })
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
    subagent_native_session_id: Option<String>,
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
        subagent_native_session_id,
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
