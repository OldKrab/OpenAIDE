use crate::agent::AgentPromptOutcome;
use crate::protocol::errors::RuntimeError;

pub(super) fn runner_registered(task_id: &str, turn_id: &str, registration_elapsed_ms: u128) {
    crate::logging::info(
        "task_primary_prompt_runner_registered",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "registration_elapsed_ms": registration_elapsed_ms,
        }),
    );
}

pub(super) fn runner_cancelled_before_running(task_id: &str, turn_id: &str, elapsed_ms: u128) {
    crate::logging::info(
        "task_primary_prompt_runner_cancelled_before_running",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "elapsed_ms": elapsed_ms,
        }),
    );
}

pub(super) fn runner_not_active_before_running(task_id: &str, turn_id: &str, elapsed_ms: u128) {
    crate::logging::warn(
        "task_primary_prompt_runner_not_active_before_running",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "elapsed_ms": elapsed_ms,
        }),
    );
}

pub(super) fn runner_mark_running_started(task_id: &str, turn_id: &str, elapsed_ms: u128) {
    crate::logging::info(
        "task_primary_prompt_runner_mark_running_started",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "elapsed_ms": elapsed_ms,
        }),
    );
}

pub(super) fn agent_invoke_returned(
    task_id: &str,
    turn_id: &str,
    result: &Result<AgentPromptOutcome, RuntimeError>,
    elapsed_ms: u128,
) {
    let outcome = match result {
        Ok(AgentPromptOutcome::EndTurn) => "end_turn",
        Ok(AgentPromptOutcome::MaxTokens) => "max_tokens",
        Ok(AgentPromptOutcome::MaxTurnRequests) => "max_turn_requests",
        Ok(AgentPromptOutcome::Refusal) => "refusal",
        Ok(AgentPromptOutcome::Cancelled) => "cancelled",
        Ok(AgentPromptOutcome::Other(_)) => "other",
        Err(_) => "error",
    };
    crate::logging::info(
        "task_primary_prompt_agent_invoke_returned",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "outcome": outcome,
            "elapsed_ms": elapsed_ms,
            "error_code": result.as_ref().err().map(RuntimeError::code),
            "error_reason": result.as_ref().err().map(RuntimeError::reason),
        }),
    );
}

pub(super) fn settlement_started(task_id: &str, turn_id: &str, elapsed_ms: u128) {
    crate::logging::info(
        "task_primary_prompt_settlement_started",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "elapsed_ms": elapsed_ms,
        }),
    );
}

pub(super) fn settlement_returned(
    task_id: &str,
    turn_id: &str,
    result: &Result<bool, RuntimeError>,
    settlement_elapsed_ms: u128,
    elapsed_ms: u128,
) {
    crate::logging::info(
        "task_primary_prompt_settlement_returned",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "outcome": if result.is_ok() { "ok" } else { "error" },
            "settlement_elapsed_ms": settlement_elapsed_ms,
            "elapsed_ms": elapsed_ms,
            "error_code": result.as_ref().err().map(RuntimeError::code),
            "error_reason": result.as_ref().err().map(RuntimeError::reason),
        }),
    );
}

pub(super) fn runner_thread_spawned(
    task_id: &str,
    turn_id: &str,
    registration_to_thread_spawned_ms: u128,
) {
    crate::logging::info(
        "task_primary_prompt_runner_thread_spawned",
        serde_json::json!({
            "task_id": task_id,
            "turn_id": turn_id,
            "registration_to_thread_spawned_ms": registration_to_thread_spawned_ms,
        }),
    );
}
