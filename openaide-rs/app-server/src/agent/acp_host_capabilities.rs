use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::acp_schema::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};
use serde_json::json;

use crate::agent::acp_elicitation_form::normalize_form;
use crate::agent::acp_elicitation_wire::{
    ElicitationContentValue, ElicitationCreateRequest, ElicitationCreateResponse, ElicitationMode,
    WireRequestId,
};
use crate::agent::acp_host::{
    create_terminal_from_host, kill_terminal_from_host, read_text_file_from_host,
    release_terminal_from_host, terminal_output_from_host, wait_for_terminal_exit_from_host,
    write_text_file_from_host,
};
use crate::agent::acp_host_terminal_ownership::AcpHostTerminalRegistry;
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::acp_update_projection::LivePromptProjection;
use crate::agent::{AgentSessionEventSink, TurnCancellation};
use crate::logging;
use crate::protocol::host::HostBridge;

pub(super) type AcpSessionEventSinkMap =
    Arc<Mutex<HashMap<String, Arc<dyn AgentSessionEventSink>>>>;
pub(super) type AcpSessionTraceMap = Arc<Mutex<HashMap<String, AcpTraceSession>>>;
pub(super) type AcpElicitationCancellationMap =
    Arc<Mutex<HashMap<WireRequestId, TurnCancellation>>>;

pub(super) type AcpSessionPromptMap = Arc<Mutex<HashMap<String, LivePromptProjection>>>;

#[derive(Clone)]
pub(super) struct AcpHostCapabilityHandlers {
    host_bridge: HostBridge,
    trace: Option<AcpTraceSession>,
    current_prompts: AcpSessionPromptMap,
    terminal_registry: AcpHostTerminalRegistry,
    session_event_sinks: AcpSessionEventSinkMap,
    session_traces: AcpSessionTraceMap,
    elicitation_cancellations: AcpElicitationCancellationMap,
}

impl AcpHostCapabilityHandlers {
    pub(super) fn new(
        host_bridge: HostBridge,
        trace: Option<AcpTraceSession>,
        current_prompts: AcpSessionPromptMap,
        terminal_registry: AcpHostTerminalRegistry,
        session_event_sinks: AcpSessionEventSinkMap,
        session_traces: AcpSessionTraceMap,
        elicitation_cancellations: AcpElicitationCancellationMap,
    ) -> Self {
        Self {
            host_bridge,
            trace,
            current_prompts,
            terminal_registry,
            session_event_sinks,
            session_traces,
            elicitation_cancellations,
        }
    }

    pub(super) async fn create_elicitation(
        &self,
        rpc_request_id: WireRequestId,
        request: ElicitationCreateRequest,
    ) -> agent_client_protocol::Result<ElicitationCreateResponse> {
        logging::info(
            "acp_elicitation_request_received",
            json!({
                "session_id": request.session_id,
                "request_id": request.request_id,
                "rpc_request_id": rpc_request_id,
                "tool_call_id": request.tool_call_id,
                "mode": request.mode,
            }),
        );
        let trace = request
            .session_id
            .as_ref()
            .and_then(|session_id| {
                self.session_traces
                    .lock()
                    .expect("ACP session trace map lock poisoned")
                    .get(session_id)
                    .cloned()
            })
            .or_else(|| self.trace.clone());
        if let Some(trace) = &trace {
            trace.record_value(
                "agent_to_client",
                "elicitation/create.request",
                json!({
                    "rpcRequestId": rpc_request_id,
                    "request": request,
                }),
            );
        }
        let response = self
            .create_elicitation_inner(rpc_request_id.clone(), request)
            .await;
        if let Some(trace) = &trace {
            match &response {
                Ok(response) => trace.record_value(
                    "client_to_agent",
                    "elicitation/create.response",
                    json!({
                        "rpcRequestId": rpc_request_id,
                        "response": response,
                    }),
                ),
                Err(error) => trace.record_value(
                    "client_to_agent",
                    "elicitation/create.error",
                    json!({
                        "rpcRequestId": rpc_request_id,
                        "error": error.to_string(),
                    }),
                ),
            }
        }
        response
    }

    pub(super) fn trace_elicitation_decode_error(
        &self,
        rpc_request_id: &WireRequestId,
        request: &serde_json::Value,
        error: &agent_client_protocol::Error,
    ) {
        let session_id = request.get("sessionId").and_then(serde_json::Value::as_str);
        logging::warn(
            "acp_elicitation_request_decode_failed",
            json!({
                "session_id": session_id,
                "rpc_request_id": rpc_request_id,
                "error": error.to_string(),
            }),
        );
        let trace = session_id
            .and_then(|session_id| {
                self.session_traces
                    .lock()
                    .expect("ACP session trace map lock poisoned")
                    .get(session_id)
                    .cloned()
            })
            .or_else(|| self.trace.clone());
        if let Some(trace) = trace {
            trace.record_value(
                "agent_to_client",
                "elicitation/create.decode_error",
                json!({
                    "rpcRequestId": rpc_request_id,
                    "request": request,
                    "error": error.to_string(),
                }),
            );
        }
    }

    async fn create_elicitation_inner(
        &self,
        rpc_request_id: WireRequestId,
        request: ElicitationCreateRequest,
    ) -> agent_client_protocol::Result<ElicitationCreateResponse> {
        if request.mode != ElicitationMode::Form {
            return Err(invalid_params("only form elicitation is supported"));
        }
        if request.session_id.is_some() == request.request_id.is_some() {
            return Err(invalid_params(
                "elicitation must have exactly one sessionId or requestId",
            ));
        }
        if request.request_id.is_some() {
            return Ok(ElicitationCreateResponse::Cancel);
        }
        let session_id = request.session_id.expect("validated session scope");
        let sink = self
            .session_event_sinks
            .lock()
            .expect("ACP session event sink lock poisoned")
            .get(&session_id)
            .cloned();
        let Some(sink) = sink else {
            return Ok(ElicitationCreateResponse::Cancel);
        };
        let Some(schema) = request.requested_schema else {
            let _ = sink.record_question_error("The Agent sent an invalid question.".to_string());
            return Err(invalid_params("form elicitation requires requestedSchema"));
        };
        let form = match normalize_form(request.message, schema) {
            Ok(form) => form,
            Err(error) => {
                let _ =
                    sink.record_question_error("The Agent sent an invalid question.".to_string());
                return Err(invalid_params(&error.to_string()));
            }
        };
        let cancellation = TurnCancellation::new();
        self.elicitation_cancellations
            .lock()
            .expect("ACP elicitation cancellation lock poisoned")
            .insert(rpc_request_id.clone(), cancellation.clone());
        let response =
            tokio::task::spawn_blocking(move || sink.request_question(form.clone(), cancellation))
                .await
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()));
        self.elicitation_cancellations
            .lock()
            .expect("ACP elicitation cancellation lock poisoned")
            .remove(&rpc_request_id);
        match response? {
            openaide_app_server_protocol::server_requests::QuestionRequestResponse::Submit {
                content,
            } => {
                let content = content.into_iter().map(|(key, value)| (key, match value {
                    openaide_app_server_protocol::server_requests::QuestionValue::String(value) => ElicitationContentValue::String(value),
                    openaide_app_server_protocol::server_requests::QuestionValue::Integer(value) => ElicitationContentValue::Integer(value),
                    openaide_app_server_protocol::server_requests::QuestionValue::Number(value) => ElicitationContentValue::Number(value),
                    openaide_app_server_protocol::server_requests::QuestionValue::Boolean(value) => ElicitationContentValue::Boolean(value),
                    openaide_app_server_protocol::server_requests::QuestionValue::StringArray(value) => ElicitationContentValue::StringArray(value),
                })).collect();
                Ok(ElicitationCreateResponse::Accept { content })
            }
            openaide_app_server_protocol::server_requests::QuestionRequestResponse::Cancel => {
                Ok(ElicitationCreateResponse::Cancel)
            }
        }
    }

    pub(super) fn cancel_elicitation(&self, request_id: &WireRequestId) {
        if let Some(cancellation) = self
            .elicitation_cancellations
            .lock()
            .expect("ACP elicitation cancellation lock poisoned")
            .get(request_id)
        {
            cancellation.cancel();
        }
    }

    pub(super) async fn request_permission(
        &self,
        request: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        let session_id = request.session_id.to_string();
        let tool_call_id = request.tool_call.tool_call_id.to_string();
        logging::info(
            "acp_permission_request_received",
            json!({
                "session_id": session_id,
                "tool_call_id": tool_call_id,
                "option_count": request.options.len(),
            }),
        );
        if let Some(trace) = &self.trace {
            trace.record(
                "agent_to_client",
                "session/request_permission.request",
                &request,
            );
        }
        let handle = self
            .current_prompts
            .lock()
            .expect("ACP active prompt lock poisoned")
            .get(&session_id)
            .cloned();
        let response = if let Some(handle) = handle {
            handle.permission_response(request).await
        } else {
            logging::warn(
                "acp_permission_no_current_prompt",
                json!({
                    "session_id": session_id,
                    "tool_call_id": tool_call_id,
                }),
            );
            Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ))
        }
        .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
        logging::info(
            "acp_permission_response_returned",
            json!({
                "session_id": session_id,
                "tool_call_id": tool_call_id,
                "outcome": acp_permission_outcome_name(&response.outcome),
            }),
        );
        if let Some(trace) = &self.trace {
            trace.record(
                "client_to_agent",
                "session/request_permission.response",
                &response,
            );
        }
        Ok(response)
    }

    pub(super) async fn read_text_file(
        &self,
        request: ReadTextFileRequest,
    ) -> agent_client_protocol::Result<ReadTextFileResponse> {
        read_text_file_from_host(self.host_bridge.clone(), request, self.trace.clone())
            .await
            .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))
    }

    pub(super) async fn write_text_file(
        &self,
        request: WriteTextFileRequest,
    ) -> agent_client_protocol::Result<WriteTextFileResponse> {
        write_text_file_from_host(self.host_bridge.clone(), request, self.trace.clone())
            .await
            .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))
    }

    pub(super) async fn create_terminal(
        &self,
        request: CreateTerminalRequest,
    ) -> agent_client_protocol::Result<CreateTerminalResponse> {
        let session_id = request.session_id.to_string();
        let permit = self
            .terminal_registry
            .begin_create(&session_id)
            .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
        let response =
            create_terminal_from_host(self.host_bridge.clone(), request, self.trace.clone())
                .await
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
        permit.complete(&response.terminal_id.to_string());
        Ok(response)
    }

    pub(super) async fn terminal_output(
        &self,
        request: TerminalOutputRequest,
    ) -> agent_client_protocol::Result<TerminalOutputResponse> {
        terminal_output_from_host(self.host_bridge.clone(), request, self.trace.clone())
            .await
            .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))
    }

    pub(super) async fn wait_for_terminal_exit(
        &self,
        request: WaitForTerminalExitRequest,
    ) -> agent_client_protocol::Result<WaitForTerminalExitResponse> {
        let session_id = request.session_id.to_string();
        let cancellation = self
            .current_prompts
            .lock()
            .expect("ACP active prompt lock poisoned")
            .get(&session_id)
            .map(|handle| handle.cancellation());
        wait_for_terminal_exit_from_host(
            self.host_bridge.clone(),
            request,
            move || {
                cancellation
                    .as_ref()
                    .map(|item| item.is_cancelled())
                    .unwrap_or(false)
            },
            self.trace.clone(),
        )
        .await
        .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))
    }

    pub(super) async fn kill_terminal(
        &self,
        request: KillTerminalRequest,
    ) -> agent_client_protocol::Result<KillTerminalResponse> {
        kill_terminal_from_host(self.host_bridge.clone(), request, self.trace.clone())
            .await
            .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))
    }

    pub(super) async fn release_terminal(
        &self,
        request: ReleaseTerminalRequest,
    ) -> agent_client_protocol::Result<ReleaseTerminalResponse> {
        let session_id = request.session_id.to_string();
        let terminal_id = request.terminal_id.to_string();
        let response =
            release_terminal_from_host(self.host_bridge.clone(), request, self.trace.clone())
                .await
                .map_err(|error| agent_client_protocol::util::internal_error(error.to_string()))?;
        self.terminal_registry.released(&session_id, &terminal_id);
        Ok(response)
    }
}

fn invalid_params(message: &str) -> agent_client_protocol::Error {
    agent_client_protocol::Error::invalid_params().data(serde_json::json!({ "details": message }))
}

fn acp_permission_outcome_name(outcome: &RequestPermissionOutcome) -> &'static str {
    match outcome {
        RequestPermissionOutcome::Selected(_) => "selected",
        RequestPermissionOutcome::Cancelled => "cancelled",
        _ => "other",
    }
}
