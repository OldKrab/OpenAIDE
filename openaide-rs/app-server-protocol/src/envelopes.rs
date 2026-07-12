use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::ProtocolError;
use crate::ids::{ClientRequestId, RequestId};
use crate::snapshot::PendingRequestScope;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientRequestEnvelope<P> {
    pub method: String,
    pub params: P,
    #[serde(default, skip_serializing_if = "RequestMeta::is_empty")]
    pub meta: RequestMeta,
}

impl<P> ClientRequestEnvelope<P> {
    pub fn new(method: impl Into<String>, params: P, meta: RequestMeta) -> Self {
        Self {
            method: method.into(),
            params,
            meta,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope<R> {
    pub result: R,
    #[serde(default, skip_serializing_if = "ResponseMeta::is_empty")]
    pub meta: ResponseMeta,
}

impl<R> ResponseEnvelope<R> {
    pub fn new(result: R, meta: ResponseMeta) -> Self {
        Self { result, meta }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEnvelope {
    pub error: ProtocolError,
    #[serde(default, skip_serializing_if = "ResponseMeta::is_empty")]
    pub meta: ResponseMeta,
}

impl ErrorEnvelope {
    pub fn new(error: ProtocolError, meta: ResponseMeta) -> Self {
        Self { error, meta }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ServerRequestEnvelope<P> {
    pub request_id: RequestId,
    pub scope: PendingRequestScope,
    pub method: String,
    pub params: P,
}

impl<P> ServerRequestEnvelope<P> {
    pub fn new(
        request_id: RequestId,
        scope: PendingRequestScope,
        method: impl Into<String>,
        params: P,
    ) -> Self {
        Self {
            request_id,
            scope,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RequestMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<ClientRequestId>,
}

impl RequestMeta {
    pub fn is_empty(&self) -> bool {
        self.client_request_id.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<ClientRequestId>,
}

impl ResponseMeta {
    pub fn is_empty(&self) -> bool {
        self.client_request_id.is_none()
    }
}

#[cfg(test)]
#[path = "envelopes_tests.rs"]
mod tests;
