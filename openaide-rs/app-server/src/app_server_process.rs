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

const LOCAL_HTTP_ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(25);

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

    /// Waits for the last App Shell client, then performs graceful shutdown.
    pub fn shutdown_after_last_client(
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
            endpoints: vec![RuntimeEndpoint {
                transport: TransportKind::LocalHttp,
                address: endpoint_address(address),
            }],
        },
    )?;
    let shutdown = start_client_liveness_expirer(gateway.clone());
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

/// Expires abandoned product clients and signals process shutdown after the last client.
fn start_client_liveness_expirer(gateway: SharedRpcGateway) -> Receiver<()> {
    let (shutdown_sender, shutdown_receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut last_native_catalog_refresh = Instant::now();
        loop {
            thread::sleep(Duration::from_secs(1));
            if gateway.has_task_navigation_subscribers()
                && last_native_catalog_refresh.elapsed() >= Duration::from_secs(60)
            {
                gateway.request_native_session_catalog_refresh();
                last_native_catalog_refresh = Instant::now();
            }
            let expired = gateway.expire_inactive_clients(AppServerTime::now());
            if !expired.is_empty() {
                crate::logging::info(
                    "local_http_clients_expired",
                    serde_json::json!({ "count": expired.len() }),
                );
            }
            if expired.iter().any(|outcome| {
                matches!(
                    outcome,
                    ClientExpiryOutcome::Expired {
                        last_client: true,
                        ..
                    }
                )
            }) {
                let _ = shutdown_sender.send(());
                return;
            }
        }
    });
    shutdown_receiver
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
