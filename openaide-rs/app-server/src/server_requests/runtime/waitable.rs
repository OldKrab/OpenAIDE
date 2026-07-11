use std::time::{Duration, Instant};

use openaide_app_server_protocol::ids::{ClientInstanceId, RequestId, TaskId};
use openaide_app_server_protocol::server_requests::{
    SecretReadParams, ShellNotificationAction, ShellNotificationLevel, ShellRevealFileParams,
    ShellShowNotificationParams, SECRET_READ, SHELL_REVEAL_FILE, SHELL_SHOW_NOTIFICATION,
};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, Delivery};
use crate::protocol::errors::RuntimeError;
use crate::server_requests::{
    OpenRequestOutcome, ServerRequestDelivery, ServerRequestDraft, ServerRequestRuntime,
};

pub(super) struct WaitableRequest {
    pub(super) result: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenWaitableRequest {
    pub request_id: RequestId,
    pub deliveries: Vec<ServerRequestDelivery>,
}

impl ServerRequestRuntime {
    pub fn open_secret_read_request(
        &self,
        client_instance_id: ClientInstanceId,
        delivery: Delivery,
        key: String,
        label: Option<String>,
        now: AppServerTime,
    ) -> Result<OpenWaitableRequest, RuntimeError> {
        let title = label.clone().unwrap_or_else(|| "Secret needed".to_string());
        let params = serde_json::to_value(SecretReadParams { key, label })
            .map_err(|error| RuntimeError::Internal(error.to_string()))?;
        self.open_waitable_client_request(
            client_instance_id,
            delivery,
            SECRET_READ,
            title,
            params,
            now,
        )
    }

    pub fn open_task_secret_read_request(
        &self,
        task_id: TaskId,
        key: String,
        label: Option<String>,
        now: AppServerTime,
    ) -> Result<OpenWaitableRequest, RuntimeError> {
        let title = label.clone().unwrap_or_else(|| "Secret needed".to_string());
        let params = serde_json::to_value(SecretReadParams { key, label })
            .map_err(|error| RuntimeError::Internal(error.to_string()))?;
        self.open_waitable_request(
            PendingRequestScope::Task { task_id },
            Vec::new(),
            SECRET_READ,
            title,
            params,
            now,
        )
    }

    pub fn open_shell_notification_request(
        &self,
        client_instance_id: ClientInstanceId,
        delivery: Delivery,
        level: ShellNotificationLevel,
        message: String,
        actions: Vec<ShellNotificationAction>,
        now: AppServerTime,
    ) -> Result<OpenWaitableRequest, RuntimeError> {
        let params = serde_json::to_value(ShellShowNotificationParams {
            level,
            message: message.clone(),
            actions,
        })
        .map_err(|error| RuntimeError::Internal(error.to_string()))?;
        self.open_waitable_client_request(
            client_instance_id,
            delivery,
            SHELL_SHOW_NOTIFICATION,
            message,
            params,
            now,
        )
    }

    pub fn open_shell_reveal_file_request(
        &self,
        client_instance_id: ClientInstanceId,
        delivery: Delivery,
        file_handle_id: String,
        label: Option<String>,
        now: AppServerTime,
    ) -> Result<OpenWaitableRequest, RuntimeError> {
        let title = label.clone().unwrap_or_else(|| "Reveal file".to_string());
        let params = serde_json::to_value(ShellRevealFileParams {
            originating_client_instance_id: client_instance_id.clone(),
            file_handle_id,
            label,
        })
        .map_err(|error| RuntimeError::Internal(error.to_string()))?;
        self.open_waitable_client_request(
            client_instance_id,
            delivery,
            SHELL_REVEAL_FILE,
            title,
            params,
            now,
        )
    }

    pub fn open_waitable_client_request(
        &self,
        client_instance_id: ClientInstanceId,
        delivery: Delivery,
        method: impl Into<String>,
        title: impl Into<String>,
        params: Value,
        now: AppServerTime,
    ) -> Result<OpenWaitableRequest, RuntimeError> {
        self.open_waitable_request(
            PendingRequestScope::Client { client_instance_id },
            vec![delivery],
            method,
            title,
            params,
            now,
        )
    }

    pub fn open_waitable_request(
        &self,
        scope: PendingRequestScope,
        deliveries: Vec<Delivery>,
        method: impl Into<String>,
        title: impl Into<String>,
        params: Value,
        now: AppServerTime,
    ) -> Result<OpenWaitableRequest, RuntimeError> {
        let draft = ServerRequestDraft {
            scope,
            method: method.into(),
            title: title.into(),
            params,
        };
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        let OpenRequestOutcome::Opened {
            snapshot,
            deliveries,
        } = inner.broker.open(draft, deliveries, now)
        else {
            return Err(RuntimeError::NotReady(
                "server request is unavailable".to_string(),
            ));
        };
        inner.waitable_requests.insert(
            snapshot.request_id.clone(),
            WaitableRequest { result: None },
        );
        Ok(OpenWaitableRequest {
            request_id: snapshot.request_id,
            deliveries,
        })
    }

    pub fn wait_client_response(
        &self,
        request_id: &RequestId,
        timeout: Duration,
    ) -> Result<Value, RuntimeError> {
        let started_at = Instant::now();
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        loop {
            let Some(waiter) = inner.waitable_requests.get_mut(request_id) else {
                return Err(RuntimeError::NotReady(
                    "server request is unavailable".to_string(),
                ));
            };
            if let Some(result) = waiter.result.take() {
                inner.waitable_requests.remove(request_id);
                return Ok(result);
            }
            let remaining = match timeout.checked_sub(started_at.elapsed()) {
                Some(remaining) if remaining > Duration::ZERO => remaining,
                _ => {
                    inner.waitable_requests.remove(request_id);
                    inner.broker.interrupt_request(request_id, AppServerTime(0));
                    return Err(RuntimeError::NotReady(
                        "server request timed out".to_string(),
                    ));
                }
            };
            let (next_inner, _) = self
                .changed
                .wait_timeout(inner, remaining.min(Duration::from_millis(50)))
                .expect("server request runtime poisoned");
            inner = next_inner;
        }
    }
}
