//! The network edge for Remote Devices: an iroh endpoint that exists only while
//! remote access is in use (ADR-0062).
//!
//! The endpoint is addressed by the App Server's public key. A connection on the
//! application protocol is served only when its key is trusted; a connection on
//! the pairing protocol may only spend the pending invite.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iroh::endpoint::{ConnectingError, Connection, ConnectionError, Incoming, IncomingAddr};
use iroh::{Endpoint, PublicKey, SecretKey};
use openaide_app_server_protocol::devices::RemoteAccessState;
use openaide_remote_core::{
    application_alpn, close, InviteRequest, Network, PairingResult, PAIRING_ALPN,
    PAIRING_MESSAGE_LIMIT,
};
use tokio::sync::mpsc;

use super::bridge::{self, BridgeTarget};
use super::network_watch::{self, describe, is_direct, path_name, watch_path, RouteTally};
use super::relay_check::{self, CheckOutcome, RelayCheck, RelayHealth};
use super::{EdgeCommand, RemoteDevices};
use crate::client_lifecycle::AppServerTime;
use crate::logging;

/// Time a removed device's connection stays open so the removal reply can be delivered.
const REMOVAL_CLOSE_GRACE: Duration = Duration::from_millis(750);
const PAIRING_STEP_TIMEOUT: Duration = Duration::from_secs(15);

/// Starts the edge on its own runtime thread. It binds nothing until remote access is in use.
pub fn start(devices: RemoteDevices, local_http: std::net::SocketAddr, auth_token: String) {
    start_on(
        devices,
        BridgeTarget {
            address: local_http,
            auth_token,
        },
        Network::Public,
    );
}

pub(crate) fn start_on(devices: RemoteDevices, target: BridgeTarget, network: Network) {
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
    network: Network,
    mut commands: mpsc::UnboundedReceiver<EdgeCommand>,
) {
    let mut endpoint: Option<Endpoint> = None;
    let connections = Connections::default();
    // Removals still owed their close; the endpoint must not stop under them.
    let mut closing: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let relay_check = RelayCheck::on(&network);
    let mut check_timer = tokio::time::interval_at(
        tokio::time::Instant::now() + relay_check::CHECK_INTERVAL,
        relay_check::CHECK_INTERVAL,
    );
    check_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // At most one check at a time, against the endpoint bound when it started.
    let mut check: Option<RunningCheck> = None;
    let mut health = RelayHealth::default();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                None => break,
                Some(EdgeCommand::Reconcile) => {
                    if reconcile(&devices, &network, &mut endpoint, &mut closing).await {
                        // A check of the endpoint just replaced says nothing about this one.
                        if let Some(running) = check.take() {
                            running.task.abort();
                        }
                        health = RelayHealth::default();
                    }
                }
                Some(EdgeCommand::Disconnect(device)) => {
                    let removed = connections
                        .lock()
                        .expect("remote connections lock poisoned")
                        .remove(&device)
                        .unwrap_or_default();
                    // The removal reply may travel over the very connection being closed,
                    // for example when a phone removes itself. Closing at once would drop
                    // that reply and leave the device's Frontend waiting forever.
                    closing.retain(|task| !task.is_finished());
                    closing.push(tokio::spawn(async move {
                        tokio::time::sleep(REMOVAL_CLOSE_GRACE).await;
                        for connection in removed {
                            connection.close(close::REMOVED.into(), b"removed");
                        }
                    }));
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
            _ = check_timer.tick(), if check.is_none() => {
                if let (Some(relay_check), Some(bound)) = (&relay_check, &endpoint) {
                    let (relay_check, address) = (relay_check.clone(), bound.addr());
                    check = Some(RunningCheck {
                        started: Instant::now(),
                        task: tokio::spawn(async move { relay_check.check(&address).await }),
                    });
                }
            }
            finished = finish(&mut check) => {
                let started = check.take().map(|running| running.started);
                let outcome = finished.unwrap_or(CheckOutcome::Skipped("task"));
                let failures_before = health.failures();
                let rebind = health.record(outcome);
                log_check(outcome, failures_before, health.failures(), started, rebind);
                if rebind {
                    rebind_edge(&devices, &network, &mut endpoint, &mut closing).await;
                }
            }
        }
    }
    if let Some(endpoint) = endpoint {
        endpoint.close().await;
    }
}

struct RunningCheck {
    started: Instant,
    task: tokio::task::JoinHandle<CheckOutcome>,
}

/// The outcome of the running check; never ready while none runs.
async fn finish(check: &mut Option<RunningCheck>) -> Result<CheckOutcome, tokio::task::JoinError> {
    match check {
        Some(running) => (&mut running.task).await,
        None => std::future::pending().await,
    }
}

/// Healthy checks stay quiet: a check is logged when it does not reach the edge, and
/// when the first one after a failure does.
fn log_check(
    outcome: CheckOutcome,
    failures_before: u32,
    failures: u32,
    started: Option<Instant>,
    rebind: bool,
) {
    if outcome == CheckOutcome::Reached && failures_before == 0 {
        return;
    }
    let fields = serde_json::json!({
        "outcome": outcome.name(),
        "stage": outcome.stage(),
        "failures": failures,
        "rebind": rebind,
        "duration_ms": started.map(elapsed_ms),
    });
    match outcome {
        CheckOutcome::Reached => logging::info("remote_devices_relay_check", fields),
        _ => logging::warn("remote_devices_relay_check", fields),
    }
}

/// Binds the edge again with the same identity, ending its connections; devices
/// reconnect. A wedged relay is left behind with the old endpoint.
async fn rebind_edge(
    devices: &RemoteDevices,
    network: &Network,
    endpoint: &mut Option<Endpoint>,
    closing: &mut Vec<tokio::task::JoinHandle<()>>,
) {
    let Some(old) = endpoint.take() else { return };
    let started = Instant::now();
    logging::warn("remote_devices_endpoint_rebinding", serde_json::json!({}));
    let closed = tokio::time::timeout(relay_check::CLOSE_TIMEOUT, old.close())
        .await
        .is_ok();
    reconcile(devices, network, endpoint, closing).await;
    let fields = serde_json::json!({
        "outcome": if endpoint.is_some() { "ok" } else { "failed" },
        "old_closed": closed,
        "duration_ms": elapsed_ms(started),
    });
    if endpoint.is_some() {
        logging::info("remote_devices_endpoint_rebound", fields);
    } else {
        logging::error("remote_devices_endpoint_rebound", fields);
    }
}

async fn accept(endpoint: &Option<Endpoint>) -> Option<Incoming> {
    match endpoint {
        Some(endpoint) => endpoint.accept().await,
        None => std::future::pending().await,
    }
}

/// Makes the endpoint match the trust state: bound with the current identity
/// while remote access is in use, and absent otherwise. Returns whether it changed.
async fn reconcile(
    devices: &RemoteDevices,
    network: &Network,
    endpoint: &mut Option<Endpoint>,
    closing: &mut Vec<tokio::task::JoinHandle<()>>,
) -> bool {
    let wanted = devices.endpoint_secret_key();
    let current = endpoint.as_ref().map(Endpoint::id);
    if current == wanted.as_ref().map(SecretKey::public) {
        return false;
    }
    if let Some(endpoint) = endpoint.take() {
        // Removing the last device stops the endpoint. Stopping first would end
        // that device's connection without the reason, and it would report an
        // unreachable computer instead of its own removal.
        for task in closing.drain(..) {
            let _ = task.await;
        }
        endpoint.close().await;
        logging::info("remote_devices_endpoint_stopped", serde_json::json!({}));
    }
    let Some(secret_key) = wanted else {
        devices.set_access(RemoteAccessState::Off);
        return true;
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
            relay_check::watch_home_relay(&bound);
            relay_check::watch_rate_limits(&bound);
            network_watch::watch_net_report(&bound);
            *endpoint = Some(bound);
            devices.set_access(RemoteAccessState::On);
        }
        Err(class) => {
            logging::error(
                "remote_devices_endpoint_started",
                serde_json::json!({
                    "outcome": "failed",
                    "duration_ms": elapsed_ms(started),
                    "error_kind": class,
                }),
            );
            devices.set_access(RemoteAccessState::Failed);
        }
    }
    true
}

pub(crate) async fn bind(
    secret_key: SecretKey,
    network: &Network,
) -> Result<Endpoint, &'static str> {
    let alpns = vec![
        application_alpn(),
        PAIRING_ALPN.to_vec(),
        relay_check::RELAY_CHECK_ALPN.to_vec(),
    ];
    let endpoint = network
        .builder()
        .secret_key(secret_key)
        .alpns(alpns)
        .bind()
        .await
        .map_err(|_| "bind")?;
    network.on_bound(&endpoint);
    Ok(endpoint)
}

async fn handle_incoming(
    incoming: Incoming,
    devices: RemoteDevices,
    target: BridgeTarget,
    connections: Connections,
) {
    let started = Instant::now();
    // Read before the handshake consumes `incoming`: a failure can then be tied to its route and,
    // over a relay, to the device that sent it.
    let origin = IncomingOrigin::of(&incoming.remote_addr(), &devices);
    let connection = match incoming.await {
        Ok(connection) => connection,
        Err(error) => {
            logging::info(
                "remote_device_handshake_failed",
                serde_json::json!({
                    "route": origin.route,
                    "device": origin.device,
                    "trusted": origin.trusted,
                    "error_kind": connecting_error_kind(&error),
                    "transport_code": connecting_transport_code(&error),
                    "duration_ms": started.elapsed().as_millis() as u64,
                }),
            );
            return;
        }
    };
    if connection.alpn() == PAIRING_ALPN {
        accept_invite(connection, devices).await;
    } else if connection.alpn() == application_alpn() {
        serve_device(connection, devices, target, connections).await;
    } else if connection.alpn() == relay_check::RELAY_CHECK_ALPN {
        relay_check::answer(connection);
    } else {
        connection.close(close::UNKNOWN_PROTOCOL.into(), b"unknown protocol");
    }
}

/// What an incoming connection reveals about its sender before the handshake. Only a relayed
/// connection names its endpoint; a direct one is known by address alone, which stays out of logs.
#[derive(Debug, PartialEq, Eq)]
struct IncomingOrigin {
    /// Named `route` because the log redacts every field called `path`.
    route: &'static str,
    device: Option<String>,
    trusted: Option<bool>,
}

impl IncomingOrigin {
    fn of(address: &IncomingAddr, devices: &RemoteDevices) -> Self {
        match address {
            IncomingAddr::Ip(_) => Self {
                route: "direct",
                device: None,
                trusted: None,
            },
            IncomingAddr::Relay { endpoint_id, .. } => Self {
                route: "relayed",
                device: Some(endpoint_id.fmt_short().to_string()),
                trusted: Some(devices.is_trusted(endpoint_id)),
            },
            _ => Self {
                route: "custom",
                device: None,
                trusted: None,
            },
        }
    }
}

/// A stable name for why an incoming connection failed. The errors' own text can carry reasons
/// sent by the peer, so it stays out of the logs.
fn connecting_error_kind(error: &ConnectingError) -> &'static str {
    match error {
        ConnectingError::ConnectionError { source, .. } => connection_error_kind(source),
        ConnectingError::HandshakeFailure { .. } => "handshake_failure",
        ConnectingError::LocallyRejected { .. } => "locally_rejected",
        _ => "internal",
    }
}

fn connection_error_kind(error: &ConnectionError) -> &'static str {
    match error {
        ConnectionError::VersionMismatch => "version_mismatch",
        ConnectionError::TransportError(_) => "transport_error",
        ConnectionError::ConnectionClosed(_) => "connection_closed",
        ConnectionError::ApplicationClosed(_) => "application_closed",
        ConnectionError::Reset => "reset",
        ConnectionError::TimedOut => "timed_out",
        ConnectionError::LocallyClosed => "locally_closed",
        ConnectionError::CidsExhausted => "cids_exhausted",
    }
}

fn connecting_transport_code(error: &ConnectingError) -> Option<u64> {
    match error {
        ConnectingError::ConnectionError { source, .. } => transport_code(source),
        _ => None,
    }
}

/// The QUIC number behind a transport failure, which tells a TLS alert (0x100 plus the alert) from
/// a protocol violation. The reason beside it is free-form, so it stays out of the logs.
fn transport_code(error: &ConnectionError) -> Option<u64> {
    match error {
        ConnectionError::TransportError(error) => Some(error.code.into()),
        _ => None,
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
        connection.close(close::UNTRUSTED.into(), b"untrusted");
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
        // Named `route` because the log redacts every field called `path`.
        serde_json::json!({ "device": device_label, "route": path_name(&initial_path) }),
    );
    let tally = Arc::new(Mutex::new(RouteTally::new(started)));
    tally
        .lock()
        .expect("route tally lock poisoned")
        .observe(is_direct(&initial_path), started);
    devices.device_connected(&device, initial_path);
    let path_watch = tokio::spawn(watch_path(
        connection.clone(),
        devices.clone(),
        tally.clone(),
    ));

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
        let stalled_connection = connection.clone();
        let stream = streams;
        tokio::spawn(async move {
            let on_head = |head: &bridge::TrustedHead| {
                if let Some(connection_id) = &head.connection_id {
                    devices.note_protocol_connection(&device, connection_id);
                }
            };
            let on_stall =
                |stall| log_write_stall(&stalled_connection, &device_label, stream, stall);
            let result = bridge::forward(recv, send, &target, on_head, on_stall).await;
            if let Err(error) = result {
                // A closed stream is how a device abandons a request; only the
                // other classes point at a fault.
                if !matches!(error, bridge::BridgeError::Stream) {
                    logging::warn(
                        "remote_device_stream_failed",
                        serde_json::json!({
                            "device": device_label,
                            "error_kind": error.class(),
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
    let mut fields = serde_json::json!({
        "device": device_label,
        "duration_ms": elapsed_ms(started),
        "streams": streams,
    });
    // How long the connection was direct shows whether hole punching ever succeeded.
    let route = tally
        .lock()
        .expect("route tally lock poisoned")
        .fields(Instant::now());
    if let (Some(fields), serde_json::Value::Object(route)) = (fields.as_object_mut(), route) {
        fields.extend(route);
    }
    logging::info("remote_device_disconnected", fields);
}

/// Logs a response write that waits on the device, with the QUIC figures that
/// tell a dead path (nothing received, losses climbing) from a device that is
/// connected but not reading. Comparing the start and end records shows which.
fn log_write_stall(
    connection: &Connection,
    device_label: &str,
    stream: u64,
    stall: bridge::WriteStall,
) {
    let stats = connection.stats();
    let paths = connection.paths();
    let selected = paths.iter().find(|path| path.is_selected());
    let mut fields = serde_json::json!({
        "device": device_label,
        "stream": stream,
        "route": path_name(&describe(connection)),
        "rtt_ms": selected.as_ref().map(|path| u64::try_from(path.rtt().as_millis()).unwrap_or(u64::MAX)),
        "cwnd": selected.as_ref().map(|path| path.stats().cwnd),
        "lost_packets": stats.lost_packets,
        "rx_datagrams": stats.udp_rx.datagrams,
        "tx_datagrams": stats.udp_tx.datagrams,
    });
    match stall {
        bridge::WriteStall::Started => {
            logging::warn("remote_device_stream_write_stalled", fields);
        }
        bridge::WriteStall::Ended { waited, delivered } => {
            fields["outcome"] = serde_json::json!(if delivered { "delivered" } else { "failed" });
            fields["duration_ms"] =
                serde_json::json!(u64::try_from(waited.as_millis()).unwrap_or(u64::MAX));
            logging::warn("remote_device_stream_write_resumed", fields);
        }
    }
}

/// The new device connected to us and presents the invite secret.
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
        let secret = request.secret_bytes().ok_or("malformed")?;
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
        fields["error_kind"] = serde_json::json!(class);
        connection.close(close::UNTRUSTED.into(), b"refused");
        logging::warn("remote_device_invite_redeemed", fields);
    } else {
        logging::info("remote_device_invite_redeemed", fields);
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "edge_tests.rs"]
mod tests;
