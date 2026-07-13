use crate::agent::events::AgentEvent;
use crate::agent::events::{
    AgentPermissionOption, AgentPermissionOptionKind, AgentPermissionRequest, AgentToolCallRef,
};
use crate::agent::{
    AgentAuthenticateRequest, AgentEventSink, AgentListSessionsRequest, AgentLoadedSession,
    AgentProbeRequest, AgentPrompt, AgentRuntime, AgentSession, AgentSessionLoad,
    AgentSessionStart,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    AgentAuthenticateResult, AgentAuthenticateStatus, AgentListSessionsResult, AgentListedSession,
    AgentProbeCapabilities, AgentProbeResult, AgentProbeStatus, NormalizedMessage,
};

use std::sync::Arc;

#[derive(Default)]
pub struct MockAgent;

impl AgentRuntime for MockAgent {
    fn probe(&self, request: AgentProbeRequest) -> Result<AgentProbeResult, RuntimeError> {
        Ok(AgentProbeResult {
            agent_id: request.agent_id,
            status: AgentProbeStatus::Ready,
            protocol_version: "test".to_string(),
            implementation_name: Some("Mock Agent".to_string()),
            implementation_version: None,
            capabilities: vec!["Test turns".to_string()],
            typed_capabilities: AgentProbeCapabilities::default(),
            auth_methods: Vec::new(),
        })
    }

    fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        Ok(AgentAuthenticateResult {
            agent_id: request.agent_id,
            method_id: request.method_id,
            status: AgentAuthenticateStatus::Authenticated,
        })
    }

    fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        Ok(AgentListSessionsResult {
            agent_id: request.agent_id,
            sessions: vec![AgentListedSession {
                session_id: "mock-session".to_string(),
                cwd: request.cwd,
                title: Some("Mock session".to_string()),
                last_activity: Some("2026-05-18T00:00:00Z".to_string()),
                updated_at: Some("2026-05-18T00:00:00Z".to_string()),
            }],
            next_cursor: request.cursor.map(|_| "mock-next-page".to_string()),
        })
    }

    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        if request.cancellation.is_cancelled() {
            return Err(RuntimeError::InvalidParams("session cancelled".to_string()));
        }
        Ok(AgentSession::new(
            request.agent_id,
            format!("session_{}", uuid::Uuid::new_v4()),
        ))
    }

    fn load_session(&self, request: AgentSessionLoad) -> Result<AgentLoadedSession, RuntimeError> {
        if request.cancellation.is_cancelled() {
            return Err(RuntimeError::InvalidParams("session cancelled".to_string()));
        }
        Ok(AgentLoadedSession {
            session: AgentSession::new(request.agent_id, request.session_id),
            replayed_messages: vec![NormalizedMessage::AgentText {
                id: "mock-loaded-agent-message".to_string(),
                text: "Mock loaded session.".to_string(),
                created_at: "2026-05-18T00:00:00Z".to_string(),
            }],
        })
    }

    fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        if prompt.cancellation.is_cancelled() {
            return Ok(());
        }
        if should_request_permission(&prompt.text) {
            let _outcome = sink.request_permission(AgentPermissionRequest {
                request_id: format!("perm_{}", uuid::Uuid::new_v4()),
                title: "Allow workspace edit?".to_string(),
                description: Some("The agent wants to modify files for this turn.".to_string()),
                scope: Some("Workspace".to_string()),
                risk: Some("File content may change.".to_string()),
                tool_call: AgentToolCallRef {
                    tool_call_id: format!("call_{}", uuid::Uuid::new_v4()),
                    title: "Edit workspace files".to_string(),
                    kind: Some("edit".to_string()),
                },
                options: vec![
                    AgentPermissionOption {
                        option_id: "allow-once".to_string(),
                        name: "Allow once".to_string(),
                        kind: AgentPermissionOptionKind::AllowOnce,
                    },
                    AgentPermissionOption {
                        option_id: "reject-once".to_string(),
                        name: "Reject".to_string(),
                        kind: AgentPermissionOptionKind::RejectOnce,
                    },
                ],
            })?;
            if prompt.cancellation.is_cancelled() {
                return Ok(());
            }
        }
        let summary = first_words(&prompt.text, 9);
        sink.emit(AgentEvent::Text(format!("I will work on: {summary}.")))?;
        sink.emit(AgentEvent::Activity {
            title: "Checked workspace context".to_string(),
            tool_name: "mock_context".to_string(),
            output_preview: "Workspace context is available for this task.".to_string(),
        })
    }
}

fn should_request_permission(text: &str) -> bool {
    text.to_lowercase().contains("permission")
}

fn first_words(text: &str, limit: usize) -> String {
    let words = text
        .split_whitespace()
        .take(limit)
        .collect::<Vec<_>>()
        .join(" ");
    if words.is_empty() {
        "the requested task".to_string()
    } else {
        words
    }
}
