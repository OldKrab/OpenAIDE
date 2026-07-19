use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum RuntimeError {
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("task not found: {0}")]
    TaskNotFound(String),
    #[error("runtime not ready: {0}")]
    NotReady(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("agent authentication required: {0}")]
    AuthRequired(String),
    #[error("agent setup required: {0}")]
    SetupRequired(String),
    #[error("Node.js required: {0}")]
    NodeJsRequired(String),
    #[error("agent unsupported: {0}")]
    Unsupported(String),
    #[error("capability missing: {0}")]
    CapabilityMissing(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl RuntimeError {
    pub fn code(&self) -> i64 {
        match self {
            RuntimeError::InvalidParams(_) => -32602,
            RuntimeError::TaskNotFound(_) => -32005,
            RuntimeError::Conflict(_) => -32009,
            RuntimeError::NotReady(_)
            | RuntimeError::AuthRequired(_)
            | RuntimeError::SetupRequired(_)
            | RuntimeError::NodeJsRequired(_) => -32002,
            RuntimeError::CapabilityMissing(_) | RuntimeError::Unsupported(_) => -32004,
            RuntimeError::MethodNotFound(_) => -32601,
            RuntimeError::Storage(_) | RuntimeError::Internal(_) => -32603,
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            RuntimeError::InvalidParams(_) => "validation_failed",
            RuntimeError::TaskNotFound(_) => "task_not_found",
            RuntimeError::NotReady(_) => "not_ready",
            RuntimeError::Conflict(_) => "conflict",
            RuntimeError::AuthRequired(_) => "auth_required",
            RuntimeError::SetupRequired(_) => "setup_required",
            RuntimeError::NodeJsRequired(_) => "node_js_required",
            RuntimeError::Unsupported(_) => "unsupported",
            RuntimeError::CapabilityMissing(_) => "capability_missing",
            RuntimeError::MethodNotFound(_) => "method_not_found",
            RuntimeError::Storage(_) => "storage_error",
            RuntimeError::Internal(_) => "internal_error",
        }
    }

    pub fn data(&self) -> serde_json::Value {
        json!({
            "reason": self.reason(),
            "recoverable": !matches!(self, RuntimeError::InvalidParams(_)),
        })
    }
}

impl From<std::io::Error> for RuntimeError {
    fn from(value: std::io::Error) -> Self {
        RuntimeError::Storage(value.to_string())
    }
}

impl From<serde_json::Error> for RuntimeError {
    fn from(value: serde_json::Error) -> Self {
        RuntimeError::Storage(value.to_string())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl From<RuntimeError> for RpcError {
    fn from(value: RuntimeError) -> Self {
        Self {
            code: value.code(),
            message: value.to_string(),
            data: Some(value.data()),
        }
    }
}
