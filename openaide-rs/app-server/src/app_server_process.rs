use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::client_lifecycle::{AppServerTime, ClientExpiryOutcome};
use crate::protocol_edge::local_http::listener::{handle_app_stream, LocalHttpProbeListener};
use crate::protocol_edge::local_http::LocalHttpAppHandler;
use crate::protocol_edge::SharedRpcGateway;
use crate::storage_runtime::{
    EndpointRecordStore, RuntimeEndpoint, RuntimeEndpointRecord, RuntimeEndpointRecordStatus,
    StateRoot, StateRootFingerprint, TransportKind,
};
use thiserror::Error;
use uuid::Uuid;

const NATIVE_SESSION_CATALOG_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
const TASK_STORAGE_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

const LOCAL_HTTP_ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(25);
const LAST_CLIENT_DRAIN_SETTLE_INTERVAL: Duration = Duration::from_millis(250);

pub struct PublishedAppServerEndpoint {
    endpoint_records: EndpointRecordStore,
    fingerprint: StateRootFingerprint,
    server_id: String,
    auth_token: String,
    gateway: SharedRpcGateway,
    shutdown: Receiver<()>,
}

impl PublishedAppServerEndpoint {
    pub fn local_http_connection(
        &self,
    ) -> Option<crate::app_server_client::LocalHttpConnectionInfo> {
        self.endpoint_records
            .read(&self.fingerprint)
            .ok()
            .flatten()
            .and_then(|record| {
                record
                    .endpoints
                    .into_iter()
                    .find(|endpoint| endpoint.transport == TransportKind::LocalHttp)
                    .map(
                        |endpoint| crate::app_server_client::LocalHttpConnectionInfo {
                            endpoint_url: endpoint.address,
                            auth_token: record.auth_token,
                        },
                    )
            })
    }

    pub fn remove_if_current(&self) {
        let _ = self
            .endpoint_records
            .remove_if(&self.fingerprint, |record| {
                record.server_id == self.server_id && record.auth_token == self.auth_token
            });
    }

    /// Waits for the last VS Code host or an authenticated replacement request.
    pub fn shutdown_when_requested(
        &self,
    ) -> Result<crate::app_lifecycle::ShutdownCompletion, crate::protocol::errors::RuntimeError>
    {
        let _ = self.shutdown.recv();
        self.gateway.shutdown()
    }
}

impl Drop for PublishedAppServerEndpoint {
    fn drop(&mut self) {
        self.remove_if_current();
    }
}

pub fn publish_local_http_probe_endpoint(
    gateway: SharedRpcGateway,
    state_root: &StateRoot,
    runtime_root: &Path,
) -> Result<PublishedAppServerEndpoint, AppServerEndpointPublishError> {
    let listener = LocalHttpProbeListener::bind_loopback()?;
    let address = listener.local_addr()?;
    let auth_token = process_token();
    let replacement_token = process_token();
    let server_id = Uuid::new_v4().to_string();
    let probe_facts = gateway.probe_facts();
    let endpoint_records = EndpointRecordStore::new(runtime_root);
    endpoint_records.write(
        state_root.fingerprint(),
        &RuntimeEndpointRecord {
            server_id: server_id.clone(),
            state_root_fingerprint: probe_facts.state_root_fingerprint,
            pid: std::process::id(),
            protocol_version: probe_facts.protocol_version,
            app_version: probe_facts.app_version,
            status: RuntimeEndpointRecordStatus::Running,
            auth_token: auth_token.clone(),
            replacement_token: Some(replacement_token.clone()),
            endpoints: vec![RuntimeEndpoint {
                transport: TransportKind::LocalHttp,
                address: endpoint_address(address),
            }],
        },
    )?;
    let (shutdown_sender, shutdown) = mpsc::channel();
    start_client_liveness_expirer(gateway.clone(), shutdown_sender.clone());
    let endpoint = PublishedAppServerEndpoint {
        endpoint_records,
        fingerprint: state_root.fingerprint().clone(),
        server_id,
        auth_token,
        gateway: gateway.clone(),
        shutdown,
    };
    start_local_http_listener(
        listener,
        LocalHttpAppHandler::new(
            gateway,
            endpoint.auth_token.clone(),
            endpoint.server_id.clone(),
            replacement_token,
            shutdown_sender,
        ),
    );
    Ok(endpoint)
}

fn start_local_http_listener(listener: LocalHttpProbeListener, handler: LocalHttpAppHandler) {
    thread::spawn(move || loop {
        let mut stream = match listener.accept() {
            Ok(stream) => stream,
            Err(error) => {
                crate::logging::error(
                    "local_http_listener_error",
                    local_http_error_fields(&error, None),
                );
                if error.is_transient_io() {
                    thread::sleep(LOCAL_HTTP_ACCEPT_ERROR_BACKOFF);
                }
                continue;
            }
        };
        let peer = stream.peer_addr().ok();
        let handler = handler.clone();
        thread::spawn(move || {
            if let Err(error) = handle_app_stream(&mut stream, &handler) {
                if error.is_transient_io() {
                    crate::logging::info(
                        "local_http_connection_closed_transient",
                        local_http_error_fields(&error, peer),
                    );
                } else {
                    crate::logging::error(
                        "local_http_connection_error",
                        local_http_error_fields(&error, peer),
                    );
                }
            }
        });
    });
}

fn local_http_error_fields(
    error: &crate::protocol_edge::local_http::listener::LocalHttpProbeListenerError,
    peer: Option<SocketAddr>,
) -> serde_json::Value {
    let mut fields = error.diagnostic_fields();
    if let Some(peer) = peer {
        fields["peer"] = serde_json::json!(peer.to_string());
        fields["peerLoopback"] = serde_json::json!(peer.ip().is_loopback());
    }
    fields
}

/// Expires abandoned product clients and wakes the process owner after a bounded drain turn.
fn start_client_liveness_expirer(gateway: SharedRpcGateway, shutdown_sender: mpsc::Sender<()>) {
    gateway.request_task_storage_maintenance();
    thread::spawn(move || {
        let mut last_native_catalog_refresh = Instant::now();
        let mut last_task_storage_maintenance = Instant::now();
        loop {
            thread::sleep(Duration::from_secs(1));
            if gateway.has_task_navigation_subscribers()
                && native_session_catalog_refresh_due(last_native_catalog_refresh.elapsed())
            {
                gateway.request_native_session_catalog_refresh();
                last_native_catalog_refresh = Instant::now();
            }
            if task_storage_maintenance_due(last_task_storage_maintenance.elapsed()) {
                gateway.request_task_storage_maintenance();
                last_task_storage_maintenance = Instant::now();
            }
            let expired = expire_local_http_clients(&gateway, AppServerTime::now());
            if request_shutdown_after_last_client(
                &gateway,
                &expired,
                &shutdown_sender,
                LAST_CLIENT_DRAIN_SETTLE_INTERVAL,
            ) {
                return;
            }
        }
    });
}

/// Gives a compatible initialize one process turn to abort draining before teardown begins.
fn request_shutdown_after_last_client(
    gateway: &SharedRpcGateway,
    expired: &[ClientExpiryOutcome],
    shutdown_sender: &mpsc::Sender<()>,
    settle_interval: Duration,
) -> bool {
    if !expired.iter().any(|outcome| {
        matches!(
            outcome,
            ClientExpiryOutcome::Expired {
                last_client: true,
                ..
            }
        )
    }) {
        return false;
    }
    if !settle_interval.is_zero() {
        thread::sleep(settle_interval);
    }
    gateway.should_shutdown_after_last_client() && shutdown_sender.send(()).is_ok()
}

fn native_session_catalog_refresh_due(elapsed: Duration) -> bool {
    elapsed >= NATIVE_SESSION_CATALOG_REFRESH_INTERVAL
}

fn task_storage_maintenance_due(elapsed: Duration) -> bool {
    elapsed >= TASK_STORAGE_MAINTENANCE_INTERVAL
}

/// Expires abandoned client-scoped state; app_lifecycle decides the process transition.
fn expire_local_http_clients(
    gateway: &SharedRpcGateway,
    now: AppServerTime,
) -> Vec<ClientExpiryOutcome> {
    let expired = gateway.expire_inactive_clients(now);
    if !expired.is_empty() {
        crate::logging::info(
            "local_http_clients_expired",
            serde_json::json!({ "count": expired.len() }),
        );
    }
    expired
}

fn endpoint_address(address: SocketAddr) -> String {
    format!("http://{address}/probe")
}

fn process_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[derive(Debug, Error)]
pub enum AppServerEndpointPublishError {
    #[error(transparent)]
    Listener(#[from] crate::protocol_edge::local_http::listener::LocalHttpProbeListenerError),
    #[error(transparent)]
    EndpointRecord(#[from] crate::storage_runtime::EndpointRecordStoreError),
}

#[cfg(test)]
#[path = "app_server_process_tests.rs"]
mod tests;
