use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::snapshot::TaskSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recoverable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ErrorTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolErrorCode {
    InvalidRequest,
    NotInitialized,
    Unauthorized,
    NotFound,
    Conflict,
    ValidationFailed,
    AttachmentHandleInvalid,
    CapabilityUnavailable,
    RequestAlreadyResolved,
    ServerStopping,
    StaleCursor,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Current authoritative render state when resolving the targeted field requires it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<Box<TaskSnapshot>>,
}
