use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use super::errors::{RpcError, RuntimeError};
use super::jsonrpc::RpcId;

const DEFAULT_HOST_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

type HostResponse = Result<Value, RpcError>;

#[derive(Debug, Clone, Serialize)]
pub struct HostRequest {
    pub jsonrpc: &'static str,
    pub id: RpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Clone)]
pub struct HostBridge {
    inner: Arc<HostBridgeInner>,
}

struct HostBridgeInner {
    sender: Option<mpsc::Sender<HostRequest>>,
    pending: Mutex<HashMap<RpcId, mpsc::Sender<HostResponse>>>,
    next_id: AtomicU64,
    default_timeout: Duration,
}

impl Default for HostBridge {
    fn default() -> Self {
        Self::disabled()
    }
}

impl HostBridge {
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(HostBridgeInner {
                sender: None,
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                default_timeout: DEFAULT_HOST_REQUEST_TIMEOUT,
            }),
        }
    }

    pub fn channel() -> (Self, mpsc::Receiver<HostRequest>) {
        Self::channel_with_timeout(DEFAULT_HOST_REQUEST_TIMEOUT)
    }

    pub fn channel_with_timeout(timeout: Duration) -> (Self, mpsc::Receiver<HostRequest>) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                inner: Arc::new(HostBridgeInner {
                    sender: Some(sender),
                    pending: Mutex::new(HashMap::new()),
                    next_id: AtomicU64::new(1),
                    default_timeout: timeout,
                }),
            },
            receiver,
        )
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.sender.is_some()
    }

    pub fn request(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<Value, RuntimeError> {
        self.request_until(method, params, Some(self.inner.default_timeout), || false)
    }

    pub fn request_with_timeout(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
        timeout: Option<Duration>,
    ) -> Result<Value, RuntimeError> {
        self.request_until(method, params, timeout, || false)
    }

    pub fn request_until(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
        timeout: Option<Duration>,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<Value, RuntimeError> {
        if is_cancelled() {
            return Err(RuntimeError::NotReady("host request cancelled".to_string()));
        }

        let Some(sender) = &self.inner.sender else {
            return Err(RuntimeError::CapabilityMissing("host bridge".to_string()));
        };

        let id = RpcId::String(format!(
            "host_{}",
            self.inner.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        let (response_tx, response_rx) = mpsc::channel();
        self.inner
            .pending
            .lock()
            .expect("host bridge pending map poisoned")
            .insert(id.clone(), response_tx);

        let request = HostRequest {
            jsonrpc: "2.0",
            id: id.clone(),
            method: method.into(),
            params,
        };

        if sender.send(request).is_err() {
            self.remove_pending(&id);
            return Err(RuntimeError::NotReady(
                "host bridge request channel closed".to_string(),
            ));
        }

        let started_at = Instant::now();
        loop {
            if is_cancelled() {
                self.remove_pending(&id);
                return Err(RuntimeError::NotReady("host request cancelled".to_string()));
            }

            let wait_for = match timeout {
                Some(timeout) => match timeout.checked_sub(started_at.elapsed()) {
                    Some(remaining) if remaining > Duration::ZERO => {
                        remaining.min(Duration::from_millis(50))
                    }
                    _ => {
                        self.remove_pending(&id);
                        return Err(RuntimeError::NotReady("host request timed out".to_string()));
                    }
                },
                None => Duration::from_millis(50),
            };

            match response_rx.recv_timeout(wait_for) {
                Ok(Ok(value)) => return Ok(value),
                Ok(Err(error)) => return Err(host_error_to_runtime(error)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.remove_pending(&id);
                    return Err(RuntimeError::NotReady(
                        "host response channel closed".to_string(),
                    ));
                }
            }
        }
    }

    pub fn try_handle_response(&self, value: &Value) -> bool {
        let Value::Object(object) = value else {
            return false;
        };
        if object.get("method").is_some() {
            return false;
        }
        if !object.contains_key("result") && !object.contains_key("error") {
            return false;
        }
        let Some(id_value) = object.get("id") else {
            return false;
        };

        let Ok(id) = serde_json::from_value::<RpcId>(id_value.clone()) else {
            return false;
        };
        let outcome = if let Some(error) = object.get("error") {
            match serde_json::from_value::<RpcError>(error.clone()) {
                Ok(error) => Err(error),
                Err(parse_error) => Err(RpcError {
                    code: -32603,
                    message: format!("invalid host error response: {parse_error}"),
                    data: None,
                }),
            }
        } else {
            Ok(object.get("result").cloned().unwrap_or(Value::Null))
        };

        if let Some(sender) = self.remove_pending(&id) {
            let _ = sender.send(outcome);
        }
        true
    }

    fn remove_pending(&self, id: &RpcId) -> Option<mpsc::Sender<HostResponse>> {
        self.inner
            .pending
            .lock()
            .expect("host bridge pending map poisoned")
            .remove(id)
    }
}

fn host_error_to_runtime(error: RpcError) -> RuntimeError {
    match error.code {
        -32601 => RuntimeError::MethodNotFound(error.message),
        -32602 => RuntimeError::InvalidParams(error.message),
        _ => RuntimeError::Internal(format!("host request failed: {}", error.message)),
    }
}

#[cfg(test)]
mod tests;
