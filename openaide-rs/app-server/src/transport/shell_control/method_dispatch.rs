use serde_json::Value;

use crate::protocol::errors::RuntimeError;
use crate::protocol::methods;
use crate::protocol::results::{EmptyResult, HealthResult};
use crate::Runtime;

pub(super) struct MethodDispatch {
    pub(super) result: Value,
    pub(super) requested_shutdown: bool,
}

pub(super) fn dispatch_method(
    runtime: &Runtime,
    method: &str,
    _params: Option<Value>,
) -> Result<MethodDispatch, RuntimeError> {
    let (result, requested_shutdown) = match method {
        methods::RUNTIME_HEALTH => (
            to_value(HealthResult {
                status: "ready",
                version: env!("CARGO_PKG_VERSION").to_string(),
                methods: methods::shell_local_methods(),
            })?,
            false,
        ),
        methods::RUNTIME_SHUTDOWN => {
            runtime.service().shutdown()?;
            (to_value(EmptyResult {})?, true)
        }
        _ => return Err(RuntimeError::MethodNotFound(method.to_string())),
    };

    Ok(MethodDispatch {
        result,
        requested_shutdown,
    })
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, RuntimeError> {
    serde_json::to_value(value).map_err(|error| RuntimeError::Internal(error.to_string()))
}
