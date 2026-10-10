//! The network edge for Remote Devices: an iroh endpoint that exists only while
//! remote access is in use (ADR-0062).
//!
//! The endpoint is addressed by the App Server's public key. A connection on the
//! application protocol is served only when its key is trusted; a connection on
//! the pairing protocol may only spend the pending invite.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iroh::endpoint::{presets, Connection, Incoming};
use iroh::{Endpoint, PublicKey, SecretKey};
use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION;
use openaide_app_server_protocol::devices::{
    RemoteAccessState, RemoteDeviceConnection, RemoteDevicePath,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::bridge::{self, BridgeTarget};
use super::{EdgeCommand, RemoteDevices};
use crate::client_lifecycle::AppServerTime;
use crate::logging;

/// Pairing is its own protocol so an untrusted key can never reach a request handler.
const PAIRING_ALPN: &[u8] = b"openaide/pair/1";
const CLOSE_UNTRUSTED: u32 = 1;
const CLOSE_REMOVED: u32 = 2;
const CLOSE_UNKNOWN_PROTOCOL: u32 = 3;
const PAIRING_STEP_TIMEOUT: Duration = Duration::from_secs(15);
const PAIRING_MESSAGE_LIMIT: usize = 4096;
/// A device that showed a join request stays on that screen for about this long.
const ANNOUNCE_ATTEMPTS: u32 = 6;
const ANNOUNCE_RETRY_DELAY: Duration = Duration::from_secs(3);
const PATH_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// The major version is part of the name, so an incompatible device fails the
/// handshake instead of a request.
fn application_alpn() -> Vec<u8> {
    format!("openaide/app-server/{}", APP_SERVER_PROTOCOL_VERSION.major).into_bytes()
}

/// How the endpoint finds and is found by its peers.
#[derive(Clone)]
pub(crate) enum EdgeNetwork {
    /// Public relays and address lookup operated by the iroh project.
    Public,
    /// Direct addresses shared in memory, so a test needs no network service.
    #[cfg(test)]
    Local(iroh::address_lookup::MemoryLookup),
}

/// Starts the edge on its own runtime thread. It binds nothing until remote access is in use.
pub fn start(devices: RemoteDevices, local_http: std::net::SocketAddr, auth_token: String) {
    start_on(
        devices,
        BridgeTarget {
            address: local_http,
            auth_token,
        },
        EdgeNetwork::Public,
    );
}

pub(crate) fn start_on(devices: RemoteDevices, target: BridgeTarget, network: EdgeNetwork) {
    let (commands, receiver) = mpsc::unbounded_channel();
    let spawned = std::thread::Builder::new()
        .name("openaide-remote-devices".to_string())
        .spawn({
            let devices = devices.clone();
            move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_error) => {
                        logging::error("remote_devices_edge_runtime_failed", serde_json::json!({}));
                        devices.set_access(RemoteAccessState::Failed);
                        return;
                    }
                };
                runtime.block_on(run(devices, target, network, receiver));
            }
        });
    if spawned.is_err() {
        logging::error("remote_devices_edge_thread_failed", serde_json::json!({}));
        return;
    }
    devices.attach_edge(move |command| {
        // The edge thread ends only with the process.
        let _ = commands.send(command);
    });
}

type Connections = Arc<Mutex<HashMap<PublicKey, Vec<Connection>>>>;

async fn run(
    devices: RemoteDevices,
    target: BridgeTarget,
    network: EdgeNetwork,
    mut commands: mpsc::UnboundedReceiver<EdgeCommand>,
) {
    let mut endpoint: Option<Endpoint> = None;
    let connections = Connections::default();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                None => break,
                Some(EdgeCommand::Reconcile) => {
                    reconcile(&devices, &network, &mut endpoint).await;
                }
                Some(EdgeCommand::AnnounceTrust(device)) => {
                    if let Some(endpoint) = &endpoint {
                        tokio::spawn(announce_trust(
                            endpoint.clone(),
                            device,
                            devices.server_name(),
                        ));
                    }
                }
                Some(EdgeCommand::Disconnect(device)) => {
                    let removed = connections
                        .lock()
                        .expect("remote connections lock poisoned")
                        .remove(&device)
                        .unwrap_or_default();
                    for connection in removed {
                        connection.close(CLOSE_REMOVED.into(), b"removed");
                    }
                }
            },
            incoming = accept(&endpoint) => match incoming {
                Some(incoming) => {
                    tokio::spawn(handle_incoming(
                        incoming,
                        devices.clone(),
                        target.clone(),
                        connections.clone(),
                    ));
                }
                None => {
                    // The endpoint stopped on its own; the next trust change retries.
                    logging::error("remote_devices_endpoint_closed", serde_json::json!({}));
                    endpoint = None;
                    devices.set_access(RemoteAccessState::Failed);
                }
            },
        }
    }
    if let Some(endpoint) = endpoint {
        endpoint.close().await;
    }
}

async fn accept(endpoint: &Option<Endpoint>) -> Option<Incoming> {
    match endpoint {
        Some(endpoint) => endpoint.accept().await,
        None => std::future::pending().await,
    }
}

/// Makes the endpoint match the trust state: bound with the current identity
/// while remote access is in use, and absent otherwise.
async fn reconcile(
    devices: &RemoteDevices,
    network: &EdgeNetwork,
    endpoint: &mut Option<Endpoint>,
) {
    let wanted = devices.endpoint_secret_key();
    let current = endpoint.as_ref().map(Endpoint::id);
    if current == wanted.as_ref().map(SecretKey::public) {
        return;
    }
    if let Some(endpoint) = endpoint.take() {
        endpoint.close().await;
        logging::info("remote_devices_endpoint_stopped", serde_json::json!({}));
    }
    let Some(secret_key) = wanted else {
        devices.set_access(RemoteAccessState::Off);
        return;
    };
    devices.set_access(RemoteAccessState::Starting);
    let started = Instant::now();
    logging::info("remote_devices_endpoint_starting", serde_json::json!({}));
    match bind(secret_key, network).await {
        Ok(bound) => {
            logging::info(
                "remote_devices_endpoint_started",
                serde_json::json!({
                    "outcome": "ok",
                    "duration_ms": elapsed_ms(started),
                    "endpoint": bound.id().fmt_short().to_string(),
                }),
            );
            *endpoint = Some(bound);
            devices.set_access(RemoteAccessState::On);
        }
        Err(class) => {
            logging::error(
                "remote_devices_endpoint_started",
                serde_json::json!({
                    "outcome": "failed",
                    "duration_ms": elapsed_ms(started),
                    "error_class": class,
                }),
            );
            devices.set_access(RemoteAccessState::Failed);
        }
    }
}

async fn bind(secret_key: SecretKey, network: &EdgeNetwork) -> Result<Endpoint, &'static str> {
    let alpns = vec![application_alpn(), PAIRING_ALPN.to_vec()];
    match network {
        EdgeNetwork::Public => Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .alpns(alpns)
            .bind()
            .await
            .map_err(|_| "bind"),
        #[cfg(test)]
        EdgeNetwork::Local(lookup) => {
            let endpoint = Endpoint::builder(presets::Minimal)
                .secret_key(secret_key)
                .alpns(alpns)
                .address_lookup(lookup.clone())
                .bind()
                .await
                .map_err(|_| "bind")?;
            lookup.add_endpoint_info(endpoint.addr());
            Ok(endpoint)
        }
    }
}

async fn handle_incoming(
    incoming: Incoming,
    devices: RemoteDevices,
    target: BridgeTarget,
    connections: Connections,
) {
    let connection = match incoming.await {
        Ok(connection) => connection,
        Err(_error) => {
            logging::info(
                "remote_device_handshake_failed",
                serde_json::json!({ "error_class": "handshake" }),
            );
            return;
        }
    };
    if connection.alpn() == PAIRING_ALPN {
        accept_invite(connection, devices).await;
    } else if connection.alpn() == application_alpn() {
        serve_device(connection, devices, target, connections).await;
    } else {
        connection.close(CLOSE_UNKNOWN_PROTOCOL.into(), b"unknown protocol");
    }
}

/// Serves one trusted connection: each stream it opens is one request.
async fn serve_device(
    connection: Connection,
    devices: RemoteDevices,
    target: BridgeTarget,
    connections: Connections,
) {
    let device = connection.remote_id();
    let device_label = device.fmt_short().to_string();
    if !devices.is_trusted(&device) {
        logging::warn(
            "remote_device_refused",
            serde_json::json!({ "device": device_label, "reason": "untrusted" }),
        );
        connection.close(CLOSE_UNTRUSTED.into(), b"untrusted");
        return;
    }
    connections
        .lock()
        .expect("remote connections lock poisoned")
        .entry(device)
        .or_default()
        .push(connection.clone());
    let started = Instant::now();
    let initial_path = describe(&connection);
    logging::info(
        "remote_device_connected",
        serde_json::json!({ "device": device_label, "path": path_name(&initial_path) }),
    );
    devices.device_connected(&device, initial_path);
    let path_watch = tokio::spawn(watch_path(connection.clone(), devices.clone()));

    let mut streams: u64 = 0;
    while let Ok((send, recv)) = connection.accept_bi().await {
        // Removal closes the connection, but a stream may already be in flight.
        if !devices.is_trusted(&device) {
            break;
        }
        streams += 1;
        let devices = devices.clone();
        let target = target.clone();
        let device_label = device_label.clone();
        tokio::spawn(async move {
            let result = bridge::forward(recv, send, &target, |head| {
                if let Some(connection_id) = &head.connection_id {
                    devices.note_protocol_connection(&device, connection_id);
                }
            })
            .await;
            if let Err(error) = result {
                // A closed stream is how a device abandons a request; only the
                // other classes point at a fault.
                if !matches!(error, bridge::BridgeError::Stream) {
                    logging::warn(
                        "remote_device_stream_failed",
                        serde_json::json!({
                            "device": device_label,
                            "error_class": error.class(),
                        }),
                    );
                }
            }
        });
    }

    path_watch.abort();
    if let Some(open) = connections
        .lock()
        .expect("remote connections lock poisoned")
        .get_mut(&device)
    {
        open.retain(|other| other.stable_id() != connection.stable_id());
    }
    devices.device_disconnected(&device, AppServerTime::now().0);
    logging::info(
        "remote_device_disconnected",
        serde_json::json!({
            "device": device_label,
            "duration_ms": elapsed_ms(started),
            "streams": streams,
        }),
    );
}

/// Reports a move between a direct path and a relay. The loop is silent unless the path changes.
async fn watch_path(connection: Connection, devices: RemoteDevices) {
    let device = connection.remote_id();
    let mut interval = tokio::time::interval(PATH_POLL_INTERVAL);
    interval.tick().await;
    loop {
        interval.tick().await;
        devices.device_path_changed(&device, describe(&connection));
    }
}

fn describe(connection: &Connection) -> RemoteDeviceConnection {
    let paths = connection.paths();
    let selected = paths.iter().find(|path| path.is_selected());
    match selected.map(|path| path.remote_addr().clone()) {
        Some(iroh::TransportAddr::Ip(address)) => RemoteDeviceConnection {
            path: RemoteDevicePath::Direct,
            address: Some(address.ip().to_string()),
        },
        _ => RemoteDeviceConnection {
            path: RemoteDevicePath::Relayed,
            address: None,
        },
    }
}

fn path_name(connection: &RemoteDeviceConnection) -> &'static str {
    match connection.path {
        RemoteDevicePath::Direct => "direct",
        RemoteDevicePath::Relayed => "relayed",
    }
}

/// What a new device sends to spend an invite.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InviteRequest {
    /// The invite secret, base32 as it appears in the Pairing Code.
    pub secret: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// The outcome of either pairing direction, sent by the App Server.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairingResult {
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

/// Invite direction: the new device connected to us and presents the secret.
async fn accept_invite(connection: Connection, devices: RemoteDevices) {
    let device = connection.remote_id();
    let device_label = device.fmt_short().to_string();
    let started = Instant::now();
    let outcome = tokio::time::timeout(PAIRING_STEP_TIMEOUT, async {
        let (mut send, mut recv) = connection.accept_bi().await.map_err(|_| "stream")?;
        let message = recv
            .read_to_end(PAIRING_MESSAGE_LIMIT)
            .await
            .map_err(|_| "read")?;
        let request: InviteRequest = serde_json::from_slice(&message).map_err(|_| "malformed")?;
        let secret = data_encoding::BASE32_NOPAD
            .decode(request.secret.to_ascii_uppercase().as_bytes())
            .map_err(|_| "malformed")?;
        let redeemed = devices.redeem_invite(
            &device,
            &secret,
            &request.name,
            request.model.as_deref(),
            AppServerTime::now().0,
        );
        let result = PairingResult {
            trusted: redeemed.is_ok(),
            server_name: redeemed.is_ok().then(|| devices.server_name()),
        };
        let bytes = serde_json::to_vec(&result).map_err(|_| "encode")?;
        send.write_all(&bytes).await.map_err(|_| "write")?;
        send.finish().map_err(|_| "write")?;
        // Closing now could discard the answer; the device closes once it has read it.
        connection.closed().await;
        redeemed.map_err(|rejection| rejection.as_str())
    })
    .await
    .unwrap_or(Err("timeout"));
    let mut fields = serde_json::json!({
        "device": device_label,
        "outcome": if outcome.is_ok() { "trusted" } else { "refused" },
        "duration_ms": elapsed_ms(started),
    });
    if let Err(class) = outcome {
        fields["error_class"] = serde_json::json!(class);
        connection.close(CLOSE_UNTRUSTED.into(), b"refused");
        logging::warn("remote_device_invite_redeemed", fields);
    } else {
        logging::info("remote_device_invite_redeemed", fields);
    }
}

/// Join-request direction: we connect to the approved device and name ourselves.
/// It learns our key from the connection, not from anything we send.
async fn announce_trust(endpoint: Endpoint, device: PublicKey, server_name: String) {
    let device_label = device.fmt_short().to_string();
    let started = Instant::now();
    logging::info(
        "remote_device_trust_announce_started",
        serde_json::json!({ "device": device_label }),
    );
    for attempt in 1..=ANNOUNCE_ATTEMPTS {
        let result = tokio::time::timeout(PAIRING_STEP_TIMEOUT, async {
            let connection = endpoint
                .connect(device, PAIRING_ALPN)
                .await
                .map_err(|_| "connect")?;
            let (mut send, mut recv) = connection.open_bi().await.map_err(|_| "stream")?;
            let bytes = serde_json::to_vec(&PairingResult {
                trusted: true,
                server_name: Some(server_name.clone()),
            })
            .map_err(|_| "encode")?;
            send.write_all(&bytes).await.map_err(|_| "write")?;
            send.finish().map_err(|_| "write")?;
            // The device answers by finishing its side once it stored our key.
            recv.read_to_end(PAIRING_MESSAGE_LIMIT)
                .await
                .map_err(|_| "acknowledge")?;
            connection.close(0u32.into(), b"done");
            Ok::<(), &'static str>(())
        })
        .await
        .unwrap_or(Err("timeout"));
        match result {
            Ok(()) => {
                logging::info(
                    "remote_device_trust_announce_completed",
                    serde_json::json!({
                        "device": device_label,
                        "outcome": "ok",
                        "attempt": attempt,
                        "duration_ms": elapsed_ms(started),
                    }),
                );
                return;
            }
            Err(class) if attempt == ANNOUNCE_ATTEMPTS => {
                // The device stays trusted: it can still connect if it learns our key
                // another way, and the user can remove it from the device list.
                logging::warn(
                    "remote_device_trust_announce_completed",
                    serde_json::json!({
                        "device": device_label,
                        "outcome": "failed",
                        "attempt": attempt,
                        "error_class": class,
                        "duration_ms": elapsed_ms(started),
                    }),
                );
            }
            Err(class) => {
                logging::info(
                    "remote_device_trust_announce_retry",
                    serde_json::json!({
                        "device": device_label,
                        "attempt": attempt,
                        "error_class": class,
                    }),
                );
                tokio::time::sleep(ANNOUNCE_RETRY_DELAY).await;
            }
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "edge_tests.rs"]
mod tests;
