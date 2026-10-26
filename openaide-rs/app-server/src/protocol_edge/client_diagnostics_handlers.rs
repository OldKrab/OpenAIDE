//! `diagnostics/recordClientEvents`: the Frontend's lifecycle events written into this App
//! Server's log, so one log follows a wait (a Task switch, a reconnect) across both sides.
//!
//! The client is not trusted to keep its events metadata-only: each event and field must fit
//! the limits below or it is dropped, and what remains still passes the logger's redaction.

use openaide_app_server_protocol::diagnostics::{
    ClientDiagnosticEvent, ClientDiagnosticEventsParams, ClientDiagnosticEventsResult,
    ClientDiagnosticValue,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use serde_json::{json, Map, Value};

use crate::client_lifecycle::ConnectionId;
use crate::logging;

use super::{responses, GatewayOutcome, RpcGateway};

/// More than a Task switch and its follow-ups in one batch; a larger batch is cut.
pub(super) const MAX_EVENTS: usize = 64;
const MAX_FIELDS: usize = 16;
const MAX_NAME: usize = 64;
const MAX_WORD: usize = 128;

impl RpcGateway {
    pub(super) fn handle_diagnostics_record_client_events(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ClientDiagnosticEventsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let recorded = record_client_events(connection_id.as_str(), params.events);
        self.result(
            connection_id,
            id,
            meta,
            ClientDiagnosticEventsResult { recorded },
        )
    }
}

/// Writes each event that fits as `frontend_<event>`; returns how many were written.
pub(super) fn record_client_events(connection_id: &str, events: Vec<ClientDiagnosticEvent>) -> u32 {
    let total = events.len();
    let mut recorded = 0;
    for event in events.into_iter().take(MAX_EVENTS) {
        if !is_name(&event.event) {
            continue;
        }
        let mut fields = Map::new();
        for (key, value) in event
            .fields
            .into_iter()
            .filter(|(key, _)| is_name(key))
            .take(MAX_FIELDS)
        {
            let value = match value {
                ClientDiagnosticValue::Flag(flag) => json!(flag),
                ClientDiagnosticValue::Count(count) => json!(count),
                ClientDiagnosticValue::Word(word) if is_word(&word) => json!(word),
                ClientDiagnosticValue::Word(_) => continue,
            };
            fields.insert(key, value);
        }
        fields.insert("connection_id".to_string(), json!(connection_id));
        fields.insert("client_at_ms".to_string(), json!(event.at_ms));
        logging::info(&format!("frontend_{}", event.event), Value::Object(fields));
        recorded += 1;
    }
    if total > recorded as usize {
        logging::warn(
            "frontend_events_dropped",
            json!({ "connection_id": connection_id, "dropped_count": total - recorded as usize }),
        );
    }
    recorded
}

fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= MAX_NAME
        && chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

/// An identifier or a fixed word, such as a Task id or an error class; never free text.
fn is_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_WORD
        && word
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
}

#[cfg(test)]
#[path = "client_diagnostics_handlers_tests.rs"]
mod tests;
