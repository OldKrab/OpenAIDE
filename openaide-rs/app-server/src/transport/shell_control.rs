use serde_json::{json, Value};

mod codec;
mod method_dispatch;

use crate::logging;
use crate::protocol::errors::{RpcError, RuntimeError};
use crate::protocol::host::HostBridge;
use crate::protocol::jsonrpc::{RpcRequest, RpcResponse};
use crate::protocol::methods;
use crate::Runtime;

use self::codec::{parse_line_values, serialize_response};
use self::method_dispatch::dispatch_method;

pub struct ShellControlDispatcher {
    runtime: Runtime,
    host_bridge: HostBridge,
    shutdown_requested: bool,
}

impl ShellControlDispatcher {
    pub fn new(runtime: Runtime) -> Self {
        let host_bridge = runtime.host_bridge();
        Self::new_with_host(runtime, host_bridge)
    }

    pub fn new_with_host(runtime: Runtime, host_bridge: HostBridge) -> Self {
        Self {
            runtime,
            host_bridge,
            shutdown_requested: false,
        }
    }

    pub fn handle_line(&mut self, line: &str) -> Vec<String> {
        let values = match parse_line_values(line) {
            Ok(values) => values,
            Err(response) => return vec![serialize_response(response)],
        };

        values
            .into_iter()
            .filter_map(|value| self.handle_value(value))
            .map(serialize_response)
            .collect()
    }

    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    fn handle_value(&mut self, value: Value) -> Option<RpcResponse> {
        if self.host_bridge.try_handle_response(&value) {
            return None;
        }

        let request: RpcRequest = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(error) => {
                logging::warn("rpc_invalid_request", json!({ "error": error.to_string() }));
                return Some(RpcResponse::error(
                    None,
                    RpcError {
                        code: -32600,
                        message: format!("invalid request: {error}"),
                        data: None,
                    },
                ));
            }
        };

        if request.jsonrpc != "2.0" {
            logging::warn("rpc_invalid_version", json!({ "method": request.method }));
            return Some(RpcResponse::error(
                request.id,
                RpcError {
                    code: -32600,
                    message: "invalid request: jsonrpc must be 2.0".to_string(),
                    data: None,
                },
            ));
        }

        if self.shutdown_requested && request.method != methods::RUNTIME_SHUTDOWN {
            return Some(RpcResponse::error(
                request.id,
                RuntimeError::NotReady("runtime shutting down".to_string()).into(),
            ));
        }

        if request.is_notification() {
            let method = request.method;
            if let Err(error) = self.dispatch(method.as_str(), request.params) {
                logging::warn(
                    "rpc_notification_failed",
                    json!({ "method": method, "error": error.to_string() }),
                );
            }
            return None;
        }

        let id = request.id.clone();
        let method = request.method;
        match self.dispatch(method.as_str(), request.params) {
            Ok(result) => {
                logging::info("rpc_request_completed", json!({ "method": method }));
                Some(RpcResponse::success(id, result))
            }
            Err(error) => {
                logging::warn(
                    "rpc_request_failed",
                    json!({ "method": method, "error": error.to_string() }),
                );
                Some(RpcResponse::error(id, error.into()))
            }
        }
    }

    fn dispatch(&mut self, method: &str, params: Option<Value>) -> Result<Value, RuntimeError> {
        let dispatch = dispatch_method(&self.runtime, method, params)?;
        if dispatch.requested_shutdown {
            self.shutdown_requested = true;
        }
        Ok(dispatch.result)
    }
}

#[cfg(test)]
mod tests;
