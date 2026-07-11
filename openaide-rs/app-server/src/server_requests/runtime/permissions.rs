use std::collections::HashMap;

use openaide_app_server_protocol::ids::RequestId;
use serde_json::{json, Value};

use crate::agent::events::{
    AgentPermissionOptionKind, AgentPermissionOutcome, AgentPermissionRequest,
};
use crate::protocol::model::PermissionDecision;

pub(super) struct PermissionWaiter {
    pub agent_request_id: String,
    options: HashMap<String, PermissionDecision>,
    pub outcome: Option<PermissionResponse>,
}

impl PermissionWaiter {
    pub(super) fn new(request: &AgentPermissionRequest) -> Self {
        Self {
            agent_request_id: request.request_id.clone(),
            options: request
                .options
                .iter()
                .map(|option| (option.option_id.clone(), permission_decision(option.kind)))
                .collect(),
            outcome: None,
        }
    }

    pub(super) fn allows_option(&self, option_id: &str) -> bool {
        self.options.contains_key(option_id)
    }

    pub(super) fn decision_for_option(&self, option_id: &str) -> Option<PermissionDecision> {
        self.options.get(option_id).copied()
    }
}

#[derive(Debug, Clone)]
pub struct PermissionResponse {
    pub outcome: AgentPermissionOutcome,
    pub decision: Option<PermissionDecision>,
}

pub(super) fn option_id_from_result(result: &Value) -> Option<String> {
    result
        .get("optionId")
        .or_else(|| result.get("option_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(super) fn set_permission_outcome(
    waiters: &mut HashMap<RequestId, PermissionWaiter>,
    request_id: &RequestId,
    option_id: String,
) -> bool {
    if let Some(waiter) = waiters.get_mut(request_id) {
        if waiter.outcome.is_some() {
            return false;
        }
        let Some(decision) = waiter.decision_for_option(&option_id) else {
            return false;
        };
        waiter.outcome = Some(PermissionResponse {
            outcome: AgentPermissionOutcome::Selected { option_id },
            decision: Some(decision),
        });
        return true;
    }
    false
}

pub(super) fn permission_params(request: &AgentPermissionRequest) -> Value {
    json!({
        "requestId": request.request_id,
        "title": request.title,
        "description": request.description,
        "scope": request.scope,
        "risk": request.risk,
        "toolCall": {
            "id": request.tool_call.tool_call_id,
            "title": request.tool_call.title,
            "kind": request.tool_call.kind,
        },
        "options": request.options.iter().map(|option| json!({
            "optionId": option.option_id,
            "name": option.name,
            "kind": permission_option_kind(option.kind),
        })).collect::<Vec<_>>(),
    })
}

fn permission_option_kind(kind: AgentPermissionOptionKind) -> &'static str {
    match kind {
        AgentPermissionOptionKind::AllowOnce => "allowOnce",
        AgentPermissionOptionKind::AllowAlways => "allowAlways",
        AgentPermissionOptionKind::RejectOnce => "rejectOnce",
        AgentPermissionOptionKind::RejectAlways => "rejectAlways",
    }
}

fn permission_decision(kind: AgentPermissionOptionKind) -> PermissionDecision {
    match kind {
        AgentPermissionOptionKind::AllowOnce | AgentPermissionOptionKind::AllowAlways => {
            PermissionDecision::Approved
        }
        AgentPermissionOptionKind::RejectOnce | AgentPermissionOptionKind::RejectAlways => {
            PermissionDecision::Denied
        }
    }
}
