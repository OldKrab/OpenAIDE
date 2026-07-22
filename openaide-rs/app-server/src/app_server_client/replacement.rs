use serde_json::{json, Value};
use thiserror::Error;

use super::probe::exchange::local_http::LocalHttpProbeExchange;
use super::probe::exchange::{
    ClientProbeExchange, ClientProbeExchangeEndpoint, ClientProbeExchangeResponse,
};
use super::probe::EndpointProbeEndpoint;
use super::EndpointTarget;
use crate::storage_runtime::TransportKind;

pub(crate) const APP_SERVER_REPLACE_METHOD: &str = "appServer/replace";
const REPLACEMENT_REQUEST_ID: &str = "app_server_replace";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplacementRequestOutcome {
    Accepted,
    /// Releases before the private replacement endpoint existed must age out naturally.
    AwaitLegacyShutdown,
    Unreachable,
}

/// Requests graceful process replacement using a credential never exposed to webviews.
pub(crate) fn request_local_http_replacement(
    target: &EndpointTarget,
) -> Result<ReplacementRequestOutcome, ReplacementRequestError> {
    let Some(replacement_token) = target.replacement_token.as_deref() else {
        return Ok(ReplacementRequestOutcome::AwaitLegacyShutdown);
    };
    let endpoint = target
        .endpoints
        .iter()
        .find(|endpoint| endpoint.transport == TransportKind::LocalHttp)
        .ok_or_else(|| ReplacementRequestError::Unavailable {
            message: "running App Server has no LocalHttp replacement endpoint".to_string(),
        })?;
    let request = json!({
        "jsonrpc": "2.0",
        "id": REPLACEMENT_REQUEST_ID,
        "method": APP_SERVER_REPLACE_METHOD,
        "params": {},
    });
    let mut exchange = LocalHttpProbeExchange::default();
    let response = exchange
        .exchange(
            ClientProbeExchangeEndpoint {
                endpoint: EndpointProbeEndpoint {
                    endpoint,
                    auth_token: replacement_token,
                },
            },
            request,
        )
        .map_err(|error| ReplacementRequestError::Request {
            message: error.message,
        })?;
    match response {
        ClientProbeExchangeResponse::Unreachable => Ok(ReplacementRequestOutcome::Unreachable),
        ClientProbeExchangeResponse::AuthFailed => Err(ReplacementRequestError::Unauthorized),
        ClientProbeExchangeResponse::Json(value) => parse_response(value),
    }
}

fn parse_response(value: Value) -> Result<ReplacementRequestOutcome, ReplacementRequestError> {
    if value.get("jsonrpc") != Some(&Value::String("2.0".to_string()))
        || value.get("id") != Some(&Value::String(REPLACEMENT_REQUEST_ID.to_string()))
    {
        return Err(ReplacementRequestError::InvalidResponse);
    }
    if value.get("result").is_some() && value.get("error").is_none() {
        return Ok(ReplacementRequestOutcome::Accepted);
    }
    Err(ReplacementRequestError::Rejected)
}

#[derive(Debug, Error)]
pub enum ReplacementRequestError {
    #[error("App Server replacement is unavailable: {message}")]
    Unavailable { message: String },
    #[error("App Server replacement request failed: {message}")]
    Request { message: String },
    #[error("App Server rejected the private replacement credential")]
    Unauthorized,
    #[error("App Server returned an invalid replacement response")]
    InvalidResponse,
    #[error("App Server rejected the replacement request")]
    Rejected,
}
