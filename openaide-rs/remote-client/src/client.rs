//! This device's endpoint and its one connection to a paired App Server.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::endpoint::{presets, Connection, ConnectionError, RecvStream, SendStream};
use iroh::{Endpoint, PublicKey, SecretKey};
use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};

use crate::pairing_code::{clamp_label, InviteCode};

const PAIRING_ALPN: &[u8] = b"openaide/pair/1";
const PAIRING_MESSAGE_LIMIT: usize = 4096;
const PAIRING_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// The App Server completes the handshake before it checks the key, so a removed
/// device learns it from the close that follows.
const REFUSAL_WAIT: Duration = Duration::from_millis(400);
/// Close codes the App Server's edge uses.
const CLOSE_UNTRUSTED: u32 = 1;
const CLOSE_REMOVED: u32 = 2;
const CLOSE_UNKNOWN_PROTOCOL: u32 = 3;

/// Receives metadata-only lifecycle events: a stable name and safe fields.
pub type Logger = Arc<dyn Fn(&'static str, serde_json::Value) + Send + Sync>;

/// Why the paired computer cannot be used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    #[error("the computer is unreachable")]
    Unreachable,
    #[error("the computer does not trust this device")]
    NotTrusted,
    #[error("the computer runs an incompatible version")]
    Incompatible,
}

impl Failure {
    pub fn class(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::NotTrusted => "not_trusted",
            Self::Incompatible => "incompatible",
        }
    }
}

/// An App Server that trusts this device, as the user knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedServer {
    pub id: PublicKey,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InviteRequest<'a> {
    secret: String,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairingResult {
    trusted: bool,
    #[serde(default)]
    server_name: Option<String>,
}

/// The major version is part of the name, so an incompatible App Server fails
/// the handshake instead of a request.
pub fn application_alpn() -> Vec<u8> {
    format!("openaide/app-server/{}", APP_SERVER_PROTOCOL_VERSION.major).into_bytes()
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
pub async fn bind(key: SecretKey) -> Result<Endpoint, Failure> {
    Endpoint::builder(presets::N0)
        .secret_key(key)
        .bind()
        .await
        .map_err(|_| Failure::Unreachable)
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
    log("remote_pairing_invite_started", serde_json::json!({}));
    let name = clamp_label(name);
    let model = model.map(clamp_label).filter(|model| !model.is_empty());
    let exchange = async {
        let connection = endpoint
            .connect(invite.server, PAIRING_ALPN)
            .await
            .map_err(|_| Failure::Unreachable)?;
        let answer = async {
            let (mut send, mut recv) = connection.open_bi().await.ok()?;
            let request = InviteRequest {
                secret: invite.secret_text(),
                name: &name,
                model: model.as_deref(),
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
            alpn,
            connection: tokio::sync::Mutex::new(None),
            attempts: AtomicU32::new(0),
            log,
        }
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
        (self.log)(
            "remote_connect_completed",
            serde_json::json!({
                "attempt": attempt,
                "outcome": match &result {
                    Ok(connection) => path_name(connection),
                    Err(failure) => failure.class(),
                },
                "duration_ms": elapsed_ms(started),
            }),
        );
        let connection = result?;
        *current = Some(connection.clone());
        Ok(connection)
    }

    async fn connect(&self) -> Result<Connection, Failure> {
        let connecting = self.endpoint.connect(self.server, &self.alpn);
        let connection = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
            Ok(Ok(connection)) => connection,
            // An App Server of another protocol major does not offer this name.
            Ok(Err(error)) if names_no_protocol(&error.to_string()) => {
                return Err(Failure::Incompatible)
            }
            Ok(Err(_)) | Err(_) => return Err(Failure::Unreachable),
        };
        match tokio::time::timeout(REFUSAL_WAIT, connection.closed()).await {
            Ok(reason) => Err(failure_of(&reason)),
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

fn failure_of(reason: &ConnectionError) -> Failure {
    let ConnectionError::ApplicationClosed(close) = reason else {
        return Failure::Unreachable;
    };
    match u32::try_from(close.error_code.into_inner()) {
        Ok(CLOSE_UNTRUSTED | CLOSE_REMOVED) => Failure::NotTrusted,
        Ok(CLOSE_UNKNOWN_PROTOCOL) => Failure::Incompatible,
        _ => Failure::Unreachable,
    }
}

/// TLS answers an unknown application protocol with alert 120, which the
/// transport reports only as text.
// TODO: match the transport's crypto error code once iroh exposes it on the
// connect error; the wording is not a contract.
fn names_no_protocol(error: &str) -> bool {
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

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
