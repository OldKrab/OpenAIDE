use serde_json::{json, Value};

use crate::logging;
use crate::protocol::errors::RpcError;
use crate::protocol::jsonrpc::RpcResponse;

// Parse failures are returned as the complete JSON-RPC wire response so callers
// cannot accidentally discard its id, structured error, or response metadata.
#[allow(clippy::result_large_err)]
pub(super) fn parse_line_values(line: &str) -> Result<Vec<Value>, RpcResponse> {
    let parsed = serde_json::from_str::<Value>(line);
    let value = match parsed {
        Ok(value) => value,
        Err(error) => {
            logging::warn("rpc_parse_failed", json!({ "error": error.to_string() }));
            return Err(RpcResponse::error(
                None,
                RpcError {
                    code: -32700,
                    message: format!("parse error: {error}"),
                    data: None,
                },
            ));
        }
    };

    Ok(match value {
        Value::Array(values) => values,
        single => vec![single],
    })
}

pub(super) fn serialize_response(response: RpcResponse) -> String {
    serde_json::to_string(&response).unwrap_or_else(|_| {
        "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32603,\"message\":\"serialization failed\"}}"
            .to_string()
    })
}
