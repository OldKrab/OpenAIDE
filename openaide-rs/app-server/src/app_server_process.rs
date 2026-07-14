use std::net::SocketAddr;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use crate::client_lifecycle::{AppServerTime, ClientExpiryOutcome};
use crate::protocol_edge::local_http::listener::{handle_app_stream, LocalHttpProbeListener};
use crate::protocol_edge::local_http::LocalHttpAppHandler;
use crate::protocol_edge::{IdleShutdownDecision, SharedRpcGateway, ShutdownBlockers};
use crate::storage_runtime::{
    EndpointRecordStore, RuntimeEndpoint, RuntimeEndpointRecord, RuntimeEndpointRecordStatus,
    StateRoot, StateRootFingerprint, TransportKind,
};
use thiserror::Error;
use uuid::Uuid;

const LOCAL_HTTP_ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(25);

/// Remembers that the last client expired until task work settles or a client reconnects.
/// Client expiry is an edge-triggered event, while task/request settlement is not, so the
/// process loop must retain this state and re-evaluate shutdown on later ticks.
#[derive(Debug, Default)]
struct IdleShutdownMonitor {
    pending: bool,
    deferred_reported: bool,
    check_failure_reported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IdleShutdownMonitorAction {
    ShutdownNow,
    Deferred {
        blockers: ShutdownBlockers,
        should_log: bool,
    },
    AbortedByClient,
}

impl IdleShutdownMonitor {
    fn observe_expirations(&mut self, outcomes: &[ClientExpiryOutcome]) {
        if outcomes.iter().any(|outcome| {
            matches!(
                outcome,
                ClientExpiryOutcome::Expired {
                    last_client: true,
                    ..
                }
            )
        }) {
            self.pending = true;
            self.deferred_reported = false;
            self.check_failure_reported = false;
        }
    }

    fn should_check(&self) -> bool {
        self.pending
    }

    fn observe_decision(&mut self, decision: IdleShutdownDecision) -> IdleShutdownMonitorAction {
        self.check_failure_reported = false;
        match decision {
            IdleShutdownDecision::ShutdownNow => {
                // Shutdown changes the gateway to Stopping even if persistence fails. Do not
                // issue a second request and mistake AlreadyStopping for a clean release.
                self.pending = false;
                IdleShutdownMonitorAction::ShutdownNow
            }
            IdleShutdownDecision::KeepRunning {
                initialized_clients: true,
                ..
            } => {
                self.pending = false;
                self.deferred_reported = false;
                IdleShutdownMonitorAction::AbortedByClient
            }
            IdleShutdownDecision::KeepRunning {
                initialized_clients: false,
                blockers,
            } => {
                let should_log = !self.deferred_reported;
                self.deferred_reported = true;
                IdleShutdownMonitorAction::Deferred {
                    blockers,
                    should_log,
                }
            }
        }
    }

    fn observe_check_failure(&mut self) -> bool {
        let should_log = !self.check_failure_reported;
        self.check_failure_reported = true;
        should_log
    }
}

#[derive(Clone)]
pub struct PublishedAppServerEndpoint {
    endpoint_records: EndpointRecordStore,
    fingerprint: StateRootFingerprint,
    server_id: String,
    auth_token: String,
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
    let endpoint = PublishedAppServerEndpoint {
        endpoint_records,
        fingerprint: state_root.fingerprint().clone(),
        server_id,
        auth_token,
    };
    start_client_liveness_expirer(gateway.clone(), endpoint.clone());
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

fn start_client_liveness_expirer(gateway: SharedRpcGateway, endpoint: PublishedAppServerEndpoint) {
    thread::spawn(move || {
        let mut shutdown_monitor = IdleShutdownMonitor::default();
        gateway.request_native_session_catalog_refresh();
        let mut last_native_catalog_refresh = Instant::now();
        loop {
            thread::sleep(Duration::from_secs(1));
            if last_native_catalog_refresh.elapsed() >= Duration::from_secs(60) {
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
            shutdown_monitor.observe_expirations(&expired);
            if !shutdown_monitor.should_check() {
                continue;
            }

            match gateway.idle_shutdown_decision() {
                Ok(decision) => match shutdown_monitor.observe_decision(decision) {
                    IdleShutdownMonitorAction::ShutdownNow => match gateway.shutdown() {
                        Ok(crate::app_lifecycle::ShutdownCompletion::CleanRelease) => {
                            endpoint.remove_if_current();
                            crate::logging::info(
                                "local_http_shutdown_clean",
                                serde_json::json!({}),
                            );
                            std::process::exit(0);
                        }
                        Ok(
                            crate::app_lifecycle::ShutdownCompletion::UncleanLeaseExpiryRequired,
                        ) => {
                            crate::logging::error(
                                "local_http_shutdown_unclean",
                                serde_json::json!({ "error": "shutdown persistence was not coherent" }),
                            );
                        }
                        Err(error) => {
                            crate::logging::error(
                                "local_http_shutdown_failed",
                                serde_json::json!({ "error": error.to_string() }),
                            );
                        }
                    },
                    IdleShutdownMonitorAction::Deferred {
                        blockers,
                        should_log: true,
                    } => {
                        crate::logging::info(
                            "local_http_shutdown_deferred",
                            serde_json::json!({
                                "initialized_clients": false,
                                "active_turns": blockers.active_turns,
                                "pending_task_requests": blockers.pending_task_requests,
                            }),
                        );
                    }
                    IdleShutdownMonitorAction::Deferred {
                        should_log: false, ..
                    }
                    | IdleShutdownMonitorAction::AbortedByClient => {}
                },
                Err(error) => {
                    if shutdown_monitor.observe_check_failure() {
                        crate::logging::error(
                            "local_http_shutdown_check_failed",
                            serde_json::json!({ "error": error.to_string() }),
                        );
                    }
                }
            }
        }
    });
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
