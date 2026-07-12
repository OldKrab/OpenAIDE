use openaide_app_server_protocol::envelopes::{
    ErrorEnvelope, RequestMeta, ResponseEnvelope, ResponseMeta,
};
use openaide_app_server_protocol::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use serde::Serialize;

use crate::client_lifecycle::ConnectionId;
use crate::server_requests::ServerRequestDelivery;

use super::{GatewayEventDelivery, GatewayOutcome, GatewayResponse};

pub fn result<T: Serialize>(
    connection_id: ConnectionId,
    id: String,
    meta: RequestMeta,
    result: T,
) -> GatewayOutcome {
    GatewayOutcome::Respond {
        connection_id,
        id,
        response: GatewayResponse::Result(
            serde_json::to_value(ResponseEnvelope::new(
                result,
                ResponseMeta {
                    client_request_id: meta.client_request_id,
                },
            ))
            .expect("protocol response should serialize"),
        ),
        events: Vec::new(),
        server_requests: Vec::new(),
    }
}

pub fn result_with_server_requests<T: Serialize>(
    connection_id: ConnectionId,
    id: String,
    meta: RequestMeta,
    result: T,
    server_requests: Vec<ServerRequestDelivery>,
) -> GatewayOutcome {
    result_with_events_and_server_requests(
        connection_id,
        id,
        meta,
        result,
        Vec::new(),
        server_requests,
    )
}

pub fn result_with_events_and_server_requests<T: Serialize>(
    connection_id: ConnectionId,
    id: String,
    meta: RequestMeta,
    result: T,
    events: Vec<GatewayEventDelivery>,
    server_requests: Vec<ServerRequestDelivery>,
) -> GatewayOutcome {
    GatewayOutcome::Respond {
        connection_id,
        id,
        response: GatewayResponse::Result(
            serde_json::to_value(ResponseEnvelope::new(
                result,
                ResponseMeta {
                    client_request_id: meta.client_request_id,
                },
            ))
            .expect("protocol response should serialize"),
        ),
        events,
        server_requests,
    }
}

pub fn result_with_events<T: Serialize>(
    connection_id: ConnectionId,
    id: String,
    meta: RequestMeta,
    result: T,
    events: Vec<GatewayEventDelivery>,
) -> GatewayOutcome {
    GatewayOutcome::Respond {
        connection_id,
        id,
        response: GatewayResponse::Result(
            serde_json::to_value(ResponseEnvelope::new(
                result,
                ResponseMeta {
                    client_request_id: meta.client_request_id,
                },
            ))
            .expect("protocol response should serialize"),
        ),
        events,
        server_requests: Vec::new(),
    }
}

pub fn error(
    connection_id: ConnectionId,
    id: String,
    meta: RequestMeta,
    error: ProtocolError,
) -> GatewayOutcome {
    GatewayOutcome::Respond {
        connection_id,
        id,
        response: GatewayResponse::Error(ErrorEnvelope::new(
            error,
            ResponseMeta {
                client_request_id: meta.client_request_id,
            },
        )),
        events: Vec::new(),
        server_requests: Vec::new(),
    }
}

pub fn not_initialized(method: String) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::NotInitialized,
        message: "client/initialize must succeed before product requests".to_string(),
        recoverable: true,
        target: Some(ErrorTarget {
            method: Some(method),
            field: None,
        }),
    }
}

pub fn invalid_params(error: serde_json::Error) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: format!("Invalid params: {error}"),
        recoverable: false,
        target: Some(ErrorTarget {
            method: None,
            field: Some("params".to_string()),
        }),
    }
}

pub fn unsupported_method(method: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: format!("Unsupported method: {method}"),
        recoverable: false,
        target: None,
    }
}
