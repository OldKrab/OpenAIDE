use openaide_app_server_protocol::ids::TaskId;
use openaide_app_server_protocol::snapshot::PendingRequestKind;
use std::sync::OnceLock;
use uuid::Uuid;

use crate::server_requests::ServerRequestRuntime;
use crate::storage::records::{TaskAttentionEvent, TaskAttentionReason};

/// Creates one durable identity for a notification-worthy Task transition.
pub(crate) fn fresh_attention(
    reason: TaskAttentionReason,
    occurred_at: impl Into<String>,
) -> TaskAttentionEvent {
    TaskAttentionEvent::new(Uuid::new_v4().to_string(), reason, occurred_at)
}

pub(crate) fn request_attention(
    request_id: impl Into<String>,
    reason: TaskAttentionReason,
    occurred_at: impl Into<String>,
) -> TaskAttentionEvent {
    let request_id = request_id.into();
    TaskAttentionEvent::new(request_attention_event_id(&request_id), reason, occurred_at)
}

/// Selects the newest still-actionable human request and reuses its request-stable identity.
pub(crate) fn current_request_attention(
    server_requests: &ServerRequestRuntime,
    task_id: &str,
    current: Option<&TaskAttentionEvent>,
    occurred_at: impl Into<String>,
) -> Option<TaskAttentionEvent> {
    let requests: Vec<_> = server_requests
        .pending_for_task(&TaskId::from(task_id.to_string()))
        .into_iter()
        .filter(|request| {
            matches!(
                request.kind,
                PendingRequestKind::Permission | PendingRequestKind::Question
            )
        })
        .collect();
    if let Some(current) = current {
        if requests
            .iter()
            .any(|request| request_attention_matches(current, request.request_id.as_str()))
        {
            return Some(current.clone());
        }
    }
    let request = requests.into_iter().max_by(|left, right| {
        server_request_sequence(left.request_id.as_str())
            .cmp(&server_request_sequence(right.request_id.as_str()))
            .then_with(|| left.request_id.cmp(&right.request_id))
    })?;
    let reason = match request.kind {
        PendingRequestKind::Permission => TaskAttentionReason::NeedsPermission,
        PendingRequestKind::Question => TaskAttentionReason::NeedsAnswer,
        PendingRequestKind::Secret | PendingRequestKind::ShellCapability => return None,
    };
    Some(request_attention(
        request.request_id.as_str(),
        reason,
        occurred_at,
    ))
}

fn request_attention_matches(event: &TaskAttentionEvent, request_id: &str) -> bool {
    event.event_id == request_attention_event_id(request_id)
}

fn request_attention_event_id(request_id: &str) -> String {
    static PROCESS_EPOCH: OnceLock<String> = OnceLock::new();
    let epoch = PROCESS_EPOCH.get_or_init(|| Uuid::new_v4().to_string());
    format!("request:{request_id}:{epoch}")
}

fn server_request_sequence(request_id: &str) -> u64 {
    request_id
        .rsplit('-')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}
