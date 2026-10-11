//! The `remote` command's connection to the App Server on this machine: one
//! plain HTTP request per call, as a same-machine client with no window.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use openaide_app_server_protocol::client::ShellKind;
use openaide_app_server_protocol::devices::DeviceCollectionSnapshot;
use openaide_app_server_protocol::methods::{
    CLIENT_DETACH, CLIENT_HEARTBEAT, CLIENT_INITIALIZE, STATE_SUBSCRIBE,
};
use serde_json::{json, Value};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientError {
    /// Nothing answered at the published address.
    Unreachable,
    /// The App Server answered with a protocol error; `code` is its stable name.
    Refused { code: String, message: String },
    /// The answer could not be read as the protocol.
    Malformed,
}

impl ClientError {
    /// Stable, content-free name for diagnostics.
    pub(crate) fn class(&self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::Refused { .. } => "refused",
            Self::Malformed => "malformed",
        }
    }
}

/// What one answer carried: the result, and the newest device list among the
/// events delivered with it.
pub(crate) struct Answer {
    pub(crate) result: Value,
    pub(crate) devices: Option<DeviceCollectionSnapshot>,
}

pub(crate) struct LocalClient {
    address: SocketAddr,
    path: String,
    auth_token: String,
    connection_id: String,
    client_instance_id: String,
    next_request: u64,
}

impl LocalClient {
    /// Reads the loopback address the App Server published. Any other host is refused.
    pub(crate) fn new(endpoint_url: &str, auth_token: &str) -> Result<Self, ClientError> {
        let rest = endpoint_url
            .strip_prefix("http://")
            .ok_or(ClientError::Malformed)?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let address: SocketAddr = authority.parse().map_err(|_| ClientError::Malformed)?;
        if !address.ip().is_loopback() {
            return Err(ClientError::Malformed);
        }
        let instance = uuid::Uuid::new_v4().simple().to_string();
        Ok(Self {
            address,
            path: path.to_string(),
            auth_token: auth_token.to_string(),
            connection_id: format!("terminal-{instance}"),
            client_instance_id: format!("terminal-{instance}"),
            next_request: 0,
        })
    }

    pub(crate) fn initialize(&mut self) -> Result<(), ClientError> {
        self.request(
            CLIENT_INITIALIZE,
            json!({
                "clientInstanceId": self.client_instance_id,
                "shell": {
                    "kind": ShellKind::Terminal,
                    "name": "OpenAIDE",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "requestedSurface": { "kind": "home" },
                "workspaceRoots": [],
            }),
        )
        .map(|_| ())
    }

    /// Renews this client's liveness and collects what happened since the last call.
    pub(crate) fn heartbeat(&mut self) -> Result<Option<DeviceCollectionSnapshot>, ClientError> {
        self.request(CLIENT_HEARTBEAT, json!({}))
            .map(|answer| answer.devices)
    }

    pub(crate) fn devices(&mut self) -> Result<DeviceCollectionSnapshot, ClientError> {
        let answer = self.request(STATE_SUBSCRIBE, json!({ "scope": { "kind": "devices" } }))?;
        serde_json::from_value(answer.result["snapshot"]["devices"].clone())
            .map_err(|_| ClientError::Malformed)
    }

    /// Running Tasks across every Agent, read from the Settings projection.
    pub(crate) fn running_task_count(&mut self) -> Result<u64, ClientError> {
        let answer = self.request(STATE_SUBSCRIBE, json!({ "scope": { "kind": "settings" } }))?;
        Ok(sum_field(&answer.result, "runningTaskCount"))
    }

    pub(crate) fn detach(&mut self) -> Result<(), ClientError> {
        self.request(CLIENT_DETACH, json!({})).map(|_| ())
    }

    pub(crate) fn request(&mut self, method: &str, params: Value) -> Result<Answer, ClientError> {
        self.next_request += 1;
        let id = format!("terminal-{}", self.next_request);
        let body =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        let wire = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-OpenAIDE-Connection-Id: {}\r\nX-OpenAIDE-Client-Instance-Id: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.path,
            self.address,
            self.auth_token,
            self.connection_id,
            self.client_instance_id,
            body.len(),
            body,
        );
        let mut stream = TcpStream::connect_timeout(&self.address, REQUEST_TIMEOUT)
            .map_err(|_| ClientError::Unreachable)?;
        stream
            .set_read_timeout(Some(REQUEST_TIMEOUT))
            .and_then(|()| stream.set_write_timeout(Some(REQUEST_TIMEOUT)))
            .map_err(|_| ClientError::Unreachable)?;
        stream
            .write_all(wire.as_bytes())
            .map_err(|_| ClientError::Unreachable)?;
        let mut response = Vec::new();
        stream
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut response)
            .map_err(|_| ClientError::Unreachable)?;
        read_answer(&response, &id)
    }
}

/// The App Server answers with either one message or a list: the response
/// followed by the events and requests queued for this connection.
pub(crate) fn read_answer(response: &[u8], id: &str) -> Result<Answer, ClientError> {
    let text = std::str::from_utf8(response).map_err(|_| ClientError::Malformed)?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or(ClientError::Unreachable)?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or(ClientError::Malformed)?;
    if status == 401 || status == 403 {
        return Err(ClientError::Refused {
            code: "unauthorized".to_string(),
            message: String::new(),
        });
    }
    let value: Value = serde_json::from_str(body).map_err(|_| ClientError::Malformed)?;
    let messages = match value {
        Value::Array(messages) => messages,
        message => vec![message],
    };
    let mut devices = None;
    for message in &messages {
        newest_devices(message, &mut devices);
    }
    let reply = messages
        .iter()
        .find(|message| message["id"] == id)
        .ok_or(ClientError::Malformed)?;
    if let Some(error) = reply.get("error") {
        let detail = error.get("error").unwrap_or(error);
        return Err(ClientError::Refused {
            code: detail["code"].as_str().unwrap_or("unknown").to_string(),
            message: detail["message"].as_str().unwrap_or_default().to_string(),
        });
    }
    let result = reply["result"]["result"].clone();
    Ok(Answer { result, devices })
}

fn newest_devices(value: &Value, found: &mut Option<DeviceCollectionSnapshot>) {
    match value {
        Value::Object(fields) => {
            if let Some(snapshot) = fields
                .get("devices")
                .filter(|devices| devices.get("remoteAccess").is_some())
                .and_then(|devices| serde_json::from_value(devices.clone()).ok())
            {
                *found = Some(snapshot);
            }
            for field in fields.values() {
                newest_devices(field, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| newest_devices(item, found)),
        _ => {}
    }
}

fn sum_field(value: &Value, name: &str) -> u64 {
    match value {
        Value::Object(fields) => fields
            .iter()
            .map(|(key, field)| {
                if key == name {
                    field.as_u64().unwrap_or(0)
                } else {
                    sum_field(field, name)
                }
            })
            .sum(),
        Value::Array(items) => items.iter().map(|item| sum_field(item, name)).sum(),
        _ => 0,
    }
}

#[cfg(test)]
#[path = "local_client_tests.rs"]
mod tests;
