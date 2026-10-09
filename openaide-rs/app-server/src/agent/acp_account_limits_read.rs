//! Account limits that reach App Server outside a turn: the reading the Agent pushes on its
//! connection, and the one App Server asks for when it has no turn to wait for.

use std::time::{Duration, Instant};

use agent_client_protocol::{Agent, ConnectionTo, UntypedMessage};
use serde_json::{json, Value};

use crate::agent::acp_account_limits_projection::{
    project_account_limits_reading, AccountLimitsReading, ACCOUNT_LIMITS_READ_METHOD,
};
use crate::agent::acp_errors::acp_request_error;
use crate::agent::events::AgentAccountLimitsChange;
use crate::agent::status_cache::AgentAccountLimitsRecorder;
use crate::logging;
use crate::protocol::errors::RuntimeError;

/// The adapter bounds its own read at five seconds after spawning Claude; this covers both.
const READ_TIMEOUT: Duration = Duration::from_secs(15);

/// Applies a pushed reading. Nothing is recorded for an account without plan limits, so a
/// reading already shown stays until the Agent reports another.
pub(super) fn record_pushed_account_limits(
    recorder: Option<&AgentAccountLimitsRecorder>,
    params: &Value,
) {
    match project_account_limits_reading(params) {
        AccountLimitsReading::Usage(change) => {
            if let Some(recorder) = recorder {
                recorder.record(change);
            }
        }
        AccountLimitsReading::Unavailable => {}
        AccountLimitsReading::Malformed => logging::warn(
            "acp_account_limits_ignored",
            json!({ "source": "notification", "ignored": ["account_limits"] }),
        ),
    }
}

/// Asks the Agent process for a reading. An Agent without the extension answers that the
/// method is unknown, which is reported as such so the caller stops asking.
pub(super) async fn read_account_limits_on_shared_process(
    connection: &ConnectionTo<Agent>,
    agent_id: &str,
) -> Result<Option<AgentAccountLimitsChange>, RuntimeError> {
    let started_at = Instant::now();
    logging::info(
        "acp_account_limits_read_started",
        json!({ "agent_id": agent_id }),
    );
    let result = read(connection).await;
    match &result {
        Ok(reading) => logging::info(
            "acp_account_limits_read_completed",
            json!({
                "agent_id": agent_id,
                "outcome": if reading.is_some() { "read" } else { "unavailable" },
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        ),
        Err(error) => logging::warn(
            "acp_account_limits_read_failed",
            json!({
                "agent_id": agent_id,
                "duration_ms": started_at.elapsed().as_millis(),
                "error_kind": error.reason(),
            }),
        ),
    }
    result
}

async fn read(
    connection: &ConnectionTo<Agent>,
) -> Result<Option<AgentAccountLimitsChange>, RuntimeError> {
    let request = UntypedMessage::new(ACCOUNT_LIMITS_READ_METHOD, json!({}))
        .map_err(|error| acp_request_error(&error))?;
    let response = tokio::time::timeout(
        READ_TIMEOUT,
        connection.send_request_to(Agent, request).block_task(),
    )
    .await
    .map_err(|_| RuntimeError::NotReady("ACP account limits read timed out".to_string()))?
    .map_err(|error| {
        if error.code == agent_client_protocol::ErrorCode::MethodNotFound {
            RuntimeError::MethodNotFound(ACCOUNT_LIMITS_READ_METHOD.into())
        } else {
            acp_request_error(&error)
        }
    })?;
    match project_account_limits_reading(&response) {
        AccountLimitsReading::Usage(change) => Ok(Some(change)),
        AccountLimitsReading::Unavailable => Ok(None),
        AccountLimitsReading::Malformed => Err(RuntimeError::Internal(
            "ACP account limits read returned an unrecognized reading".to_string(),
        )),
    }
}
