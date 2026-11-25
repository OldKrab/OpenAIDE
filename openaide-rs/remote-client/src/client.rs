//! This device's endpoint and its one connection to a paired App Server.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use iroh::endpoint::{
    ConnectError, ConnectWithOptsError, Connection, ConnectionError, QuicTransportConfig,
    RecvStream, SendStream,
};
use iroh::{Endpoint, EndpointAddr, PublicKey, SecretKey, Watcher};
use openaide_remote_core::{
    application_alpn, clamp_label, close, InviteCode, InviteRequest, Network, OwnRelay,
    PairingResult, PAIRING_ALPN, PAIRING_MESSAGE_LIMIT,
};

const PAIRING_TIMEOUT: Duration = Duration::from_secs(20);
/// How long reaching a paired App Server may take before it counts as unreachable.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// A CORS preflight: every App Server answers it before any handler runs, so it
/// asks for an answer and nothing else.
const ROUND_TRIP_REQUEST: &[u8] = b"OPTIONS /probe HTTP/1.1\r\nContent-Length: 0\r\n\r\n";
/// The App Server completes the handshake before it checks the key, so a removed
/// device learns it from the close that follows.
pub(crate) const REFUSAL_WAIT: Duration = Duration::from_millis(400);

pub use crate::Logger;

/// Why the paired computer cannot be used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    #[error("the computer is unreachable")]
    Unreachable,
    #[error("the computer does not trust this device")]
    NotTrusted,
    #[error("the computer runs an incompatible version")]
    Incompatible,
    /// This device's own key could not be read or kept; the computer was not tried.
    #[error("this device's secure storage is unavailable")]
    Storage,
}

impl Failure {
    pub fn class(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::NotTrusted => "not_trusted",
            Self::Incompatible => "incompatible",
            Self::Storage => "storage",
        }
    }
}

/// An App Server that trusts this device, as the user knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedServer {
    pub id: PublicKey,
    pub name: String,
    /// The Own Relay the App Server is on, from its invite (ADR-0085). The device
    /// stores it with the computer, password included, and dials there.
    pub relay: Option<OwnRelay>,
}

/// Where to dial `server`. A computer on an Own Relay is dialed at that relay, which
/// the endpoint first learns along with its password (ADR-0085); iroh sends the
/// password to that relay only. Any other computer is dialed by key alone, and the
/// public address lookup finds it.
pub async fn server_address(
    endpoint: &Endpoint,
    server: PublicKey,
    relay: Option<&OwnRelay>,
) -> EndpointAddr {
    match relay {
        None => EndpointAddr::new(server),
        Some(relay) => {
            endpoint
                .insert_relay(relay.url().clone(), relay.config())
                .await;
            EndpointAddr::new(server).with_relay_url(relay.url().clone())
        }
    }
}

pub fn new_device_key() -> SecretKey {
    SecretKey::generate()
}

/// The key as text for the platform's secure storage.
pub fn device_key_text(key: &SecretKey) -> String {
    data_encoding::HEXLOWER.encode(&key.to_bytes())
}

pub fn device_key_from_text(text: &str) -> Option<SecretKey> {
    let bytes = data_encoding::HEXLOWER
        .decode(text.trim().as_bytes())
        .ok()?;
    let bytes: [u8; 32] = bytes.try_into().ok()?;
    Some(SecretKey::from_bytes(&bytes))
}

/// Binds this device's endpoint on the public relays and address lookup the App
/// Server uses. One endpoint per key: a second would compete for its relay.
///
/// Logs each time the endpoint gains or loses its home relay, for as long as it
/// lives: without a relay a remote App Server can be reached only directly.
pub async fn bind(key: SecretKey, log: Logger) -> Result<Endpoint, Failure> {
    bind_on(&Network::Public, key, log).await
}

/// Binds as [`bind`] does, on `network`; tests pass a network of their own.
pub(crate) async fn bind_on(
    network: &Network,
    key: SecretKey,
    log: Logger,
) -> Result<Endpoint, Failure> {
    let endpoint = network
        .builder()
        .secret_key(key)
        // Phones on mobile data lose the direct path as soon as a bulk send starts while
        // segmentation offload is on, and fall back to the relay (iroh issue #4647).
        // TODO: re-enable once iroh detects networks that drop segmented datagrams.
        .transport_config(
            QuicTransportConfig::builder()
                .enable_segmentation_offload(false)
                .build(),
        )
        .bind()
        .await
        .map_err(|_| Failure::Unreachable)?;
    network.on_bound(&endpoint);
    crate::watch_relay_rate_limits(&endpoint, log.clone(), "remote_endpoint_relay_rate_limited");
    crate::watch_net_report(&endpoint, log.clone(), "remote_endpoint_network_changed");
    let mut statuses = endpoint.home_relay_status();
    tokio::spawn(async move {
        let mut connected = None;
        let mut changed_at = Instant::now();
        loop {
            let now = statuses.get().iter().any(|status| status.is_connected());
            if connected != Some(now) {
                (log)(
                    "remote_endpoint_relay_changed",
                    serde_json::json!({
                        "connected": now,
                        "previous_duration_ms": connected.map(|_| elapsed_ms(changed_at)),
                    }),
                );
                connected = Some(now);
                changed_at = Instant::now();
            }
            // Ends with the last clone of the endpoint.
            if statuses.updated().await.is_err() {
                break;
            }
        }
    });
    Ok(endpoint)
}

/// Spends an invite: connects to the App Server it names and presents its secret.
pub async fn redeem(
    endpoint: &Endpoint,
    invite: &InviteCode,
    name: &str,
    model: Option<&str>,
    log: &Logger,
) -> Result<PairedServer, Failure> {
    let started = Instant::now();
    log(
        "remote_pairing_invite_started",
        serde_json::json!({ "own_relay": invite.relay.is_some() }),
    );
    let name = clamp_label(name);
    let model = model.map(clamp_label).filter(|model| !model.is_empty());
    let exchange = async {
        let address = server_address(endpoint, invite.server, invite.relay.as_ref()).await;
        let connection = endpoint
            .connect(address, PAIRING_ALPN)
            .await
            .map_err(|_| Failure::Unreachable)?;
        let answer = async {
            let (mut send, mut recv) = connection.open_bi().await.ok()?;
            let request = InviteRequest {
                secret: invite.secret_text(),
                name: name.clone(),
                model: model.clone(),
            };
            send.write_all(&serde_json::to_vec(&request).ok()?)
                .await
                .ok()?;
            send.finish().ok()?;
            recv.read_to_end(PAIRING_MESSAGE_LIMIT).await.ok()
        }
        .await;
        connection.close(0u32.into(), b"done");
        let answer = answer.ok_or(Failure::Unreachable)?;
        serde_json::from_slice::<PairingResult>(&answer).map_err(|_| Failure::Unreachable)
    };
    let result = match tokio::time::timeout(PAIRING_TIMEOUT, exchange).await {
        Ok(Ok(result)) if result.trusted => Ok(PairedServer {
            id: invite.server,
            name: clamp_label(result.server_name.as_deref().unwrap_or("")),
            relay: invite.relay.clone(),
        }),
        // A spent, expired or mistyped secret; the App Server does not say which.
        Ok(Ok(_)) => Err(Failure::NotTrusted),
        Ok(Err(failure)) => Err(failure),
        Err(_) => Err(Failure::Unreachable),
    };
    log(
        "remote_pairing_invite_completed",
        serde_json::json!({
            "outcome": match &result { Ok(_) => "trusted", Err(failure) => failure.class() },
            "duration_ms": elapsed_ms(started),
        }),
    );
    result
}

/// Keeps one connection to one paired App Server, opened or reopened on demand.
///
/// Recovery after a lost connection stays with the Frontend's resumable session:
/// it retries, and each retry asks here for a live connection.
pub struct RemoteClient {
    endpoint: Endpoint,
    server: PublicKey,
    relay: Option<OwnRelay>,
    alpn: Vec<u8>,
    connection: tokio::sync::Mutex<Option<Connection>>,
    attempts: AtomicU32,
    log: Logger,
}

impl RemoteClient {
    pub fn new(endpoint: Endpoint, server: PublicKey, log: Logger) -> Self {
        Self::with_alpn(endpoint, server, application_alpn(), log)
    }

    /// Names the application protocol explicitly; a test uses it to stand for
    /// another protocol major.
    pub fn with_alpn(endpoint: Endpoint, server: PublicKey, alpn: Vec<u8>, log: Logger) -> Self {
        Self {
            endpoint,
            server,
            relay: None,
            alpn,
            connection: tokio::sync::Mutex::new(None),
            attempts: AtomicU32::new(0),
            log,
        }
    }

    /// Dials the App Server at its Own Relay, as its invite named it (ADR-0085).
    pub fn on_relay(mut self, relay: Option<OwnRelay>) -> Self {
        self.relay = relay;
        self
    }

    pub fn server(&self) -> PublicKey {
        self.server
    }

    /// The live connection, opened when there is none or the last one ended.
    pub async fn connection(&self) -> Result<Connection, Failure> {
        // Holding the lock across the attempt makes concurrent requests share it.
        let mut current = self.connection.lock().await;
        if let Some(connection) = current.take() {
            match connection.close_reason() {
                None => {
                    *current = Some(connection.clone());
                    return Ok(connection);
                }
                // The App Server said why it ended the connection. It may stop
                // listening right after removing its last device, so a new
                // attempt would only report an unreachable computer.
                Some(reason) if failure_of(&reason) != Failure::Unreachable => {
                    let failure = failure_of(&reason);
                    (self.log)(
                        "remote_connection_ended",
                        serde_json::json!({ "error_class": failure.class() }),
                    );
                    return Err(failure);
                }
                Some(_) => {}
            }
        }
        let attempt = self.attempts.fetch_add(1, Ordering::Relaxed) + 1;
        let started = Instant::now();
        (self.log)(
            "remote_connect_started",
            serde_json::json!({ "attempt": attempt }),
        );
        let result = self.connect().await;
        let mut fields = serde_json::json!({
            "attempt": attempt,
            "outcome": match &result {
                Ok(connection) => path_name(connection),
                Err((failure, _)) => failure.class(),
            },
            "duration_ms": elapsed_ms(started),
        });
        // A failure that never reaches the App Server leaves no trace there, so
        // this side records how far the attempt got and what its endpoint could use.
        if let Err((_, stage)) = &result {
            let address = self.endpoint.addr();
            fields["stage"] = (*stage).into();
            fields["endpoint_closed"] = self.endpoint.is_closed().into();
            fields["relay_connected"] = relay_connected(&self.endpoint).into();
            fields["direct_addresses"] = address.ip_addrs().count().into();
        }
        (self.log)("remote_connect_completed", fields);
        let connection = result.map_err(|(failure, _)| failure)?;
        *current = Some(connection.clone());
        Ok(connection)
    }

    /// Fails with the reason and the stage it was reached at.
    async fn connect(&self) -> Result<Connection, (Failure, &'static str)> {
        let address = server_address(&self.endpoint, self.server, self.relay.as_ref()).await;
        let connecting = self.endpoint.connect(address, &self.alpn);
        let connection = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
            Ok(Ok(connection)) => connection,
            // An App Server of another protocol major does not offer this name.
            Ok(Err(error)) if names_no_protocol(&error.to_string()) => {
                return Err((Failure::Incompatible, "protocol"))
            }
            Ok(Err(error)) => return Err((Failure::Unreachable, connect_stage(&error))),
            Err(_) => return Err((Failure::Unreachable, "timeout")),
        };
        match tokio::time::timeout(REFUSAL_WAIT, connection.closed()).await {
            Ok(reason) => Err((failure_of(&reason), "refused")),
            Err(_) => Ok(connection),
        }
    }

    /// Opens the stream for one request, replacing a connection that died since
    /// the last request once.
    pub async fn open(&self) -> Result<(SendStream, RecvStream), Failure> {
        let mut failure = Failure::Unreachable;
        for _ in 0..2 {
            let connection = self.connection().await?;
            match connection.open_bi().await {
                Ok(stream) => return Ok(stream),
                Err(_) => {
                    failure = connection
                        .close_reason()
                        .map_or(Failure::Unreachable, |reason| failure_of(&reason));
                    self.drop_connection(&connection).await;
                }
            }
            if failure != Failure::Unreachable {
                break;
            }
        }
        Err(failure)
    }

    /// Asks the App Server for one answer over the live connection, opening one
    /// when there is none, and fails when none arrives `within` that time.
    ///
    /// A computer that stopped without closing the connection (a crash, or its
    /// network dropping) leaves it looking open until QUIC's idle timeout ends
    /// it, 15 to 30 s later; a round trip finds it now. A connection that gave
    /// no answer is dropped, so the next request opens a new one.
    pub async fn round_trip(&self, within: Duration) -> Result<(), Failure> {
        let started = Instant::now();
        (self.log)("remote_round_trip_started", serde_json::json!({}));
        let result = self.exchange_round_trip(within).await;
        let mut fields = serde_json::json!({
            "outcome": match &result {
                Ok(()) => "answered",
                Err((failure, _)) => failure.class(),
            },
            "duration_ms": elapsed_ms(started),
        });
        if let Err((_, stage)) = &result {
            fields["stage"] = (*stage).into();
        }
        (self.log)("remote_round_trip_completed", fields);
        result.map_err(|(failure, _)| failure)
    }

    /// Fails with the reason and the stage it was reached at.
    async fn exchange_round_trip(&self, within: Duration) -> Result<(), (Failure, &'static str)> {
        let connection = self
            .connection()
            .await
            .map_err(|failure| (failure, "connect"))?;
        let exchange = async {
            let (mut send, mut recv) = connection.open_bi().await.map_err(|_| "open")?;
            send.write_all(ROUND_TRIP_REQUEST)
                .await
                .map_err(|_| "send")?;
            send.finish().map_err(|_| "send")?;
            // Any HTTP response is an answer; the status does not matter here.
            let mut answer = [0u8; 5];
            recv.read_exact(&mut answer).await.map_err(|_| "answer")?;
            if &answer != b"HTTP/" {
                return Err("answer");
            }
            Ok(())
        };
        let stage = match tokio::time::timeout(within, exchange).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(stage)) => stage,
            Err(_) => "timeout",
        };
        let failure = connection
            .close_reason()
            .map_or(Failure::Unreachable, |reason| failure_of(&reason));
        self.drop_connection(&connection).await;
        Err((failure, stage))
    }

    async fn drop_connection(&self, failed: &Connection) {
        let mut current = self.connection.lock().await;
        if current
            .as_ref()
            .is_some_and(|connection| connection.stable_id() == failed.stable_id())
        {
            *current = None;
        }
    }

    /// Ends the connection when the user stops using this computer.
    pub async fn disconnect(&self) {
        if let Some(connection) = self.connection.lock().await.take() {
            connection.close(0u32.into(), b"done");
            (self.log)("remote_disconnected", serde_json::json!({}));
        }
    }
}

pub(crate) fn failure_of(reason: &ConnectionError) -> Failure {
    let ConnectionError::ApplicationClosed(close) = reason else {
        return Failure::Unreachable;
    };
    match u32::try_from(close.error_code.into_inner()) {
        Ok(close::UNTRUSTED | close::REMOVED) => Failure::NotTrusted,
        Ok(close::UNKNOWN_PROTOCOL) => Failure::Incompatible,
        _ => Failure::Unreachable,
    }
}

/// Names how far a failed attempt got, from the error's variant alone.
pub(crate) fn connect_stage(error: &ConnectError) -> &'static str {
    match error {
        ConnectError::Connect { source, .. } => match source {
            ConnectWithOptsError::NoAddress { .. } => "no_address",
            ConnectWithOptsError::EndpointClosed { .. } => "endpoint_closed",
            _ => "dial",
        },
        ConnectError::Connecting { .. } => "handshake",
        ConnectError::Connection { .. } => "connection",
        _ => "unknown",
    }
}

fn relay_connected(endpoint: &Endpoint) -> bool {
    endpoint
        .home_relay_status()
        .get()
        .iter()
        .any(|status| status.is_connected())
}

/// TLS answers an unknown application protocol with alert 120, which the
/// transport reports only as text.
// TODO: match the transport's crypto error code once iroh exposes it on the
// connect error; the wording is not a contract.
pub(crate) fn names_no_protocol(error: &str) -> bool {
    error.contains("error 120") || error.contains("any known protocol")
}

fn path_name(connection: &Connection) -> &'static str {
    let paths = connection.paths();
    match paths
        .iter()
        .find(|path| path.is_selected())
        .map(|path| path.remote_addr().clone())
    {
        Some(iroh::TransportAddr::Ip(_)) => "direct",
        _ => "relayed",
    }
}

pub(crate) fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
