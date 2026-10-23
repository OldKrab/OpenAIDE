//! The loopback origin a shell's Frontend talks to while the App Server is on
//! another computer.
//!
//! The Frontend keeps the HTTP it speaks to a same-machine App Server. Every
//! process on this computer can reach loopback, so a request is carried only
//! when it holds the token this process gave its own Frontend. That token stops
//! here: the App Server trusts the connection's key and ignores credentials.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::client::{Failure, Logger, RemoteClient};

const MAX_HEAD_BYTES: usize = 16 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(30);
const HEAD_END: &[u8] = b"\r\n\r\n";

/// A running gateway: where it listens and the token its Frontend must send.
pub struct Gateway {
    pub address: SocketAddr,
    pub token: String,
}

impl Gateway {
    /// Listens on a free loopback port for the rest of the process.
    ///
    /// `on_failure` hears the failures a retry cannot fix: this device was
    /// removed, or the two sides run incompatible versions.
    pub async fn start(
        client: Arc<RemoteClient>,
        log: Logger,
        on_failure: impl Fn(Failure) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        // Two fresh keys are 64 hex characters nobody else on this computer saw.
        let token = [(); 2]
            .map(|()| data_encoding::HEXLOWER.encode(&iroh::SecretKey::generate().to_bytes()[..16]))
            .concat();
        let shared = Arc::new(Shared {
            client,
            token: token.clone(),
            log,
            on_failure: Box::new(on_failure),
            requests: AtomicU64::new(0),
        });
        (shared.log)("remote_gateway_listening", serde_json::json!({}));
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, _)) => {
                        tokio::spawn(serve(socket, shared.clone()));
                    }
                    Err(_) => {
                        (shared.log)("remote_gateway_stopped", serde_json::json!({}));
                        break;
                    }
                }
            }
        });
        Ok(Self { address, token })
    }
}

struct Shared {
    client: Arc<RemoteClient>,
    token: String,
    log: Logger,
    on_failure: Box<dyn Fn(Failure) + Send + Sync>,
    requests: AtomicU64,
}

async fn serve(mut socket: TcpStream, shared: Arc<Shared>) {
    let _ = socket.set_nodelay(true);
    let Ok(Some((head, body_start))) =
        tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut socket)).await
    else {
        return;
    };
    let forwarded = match forwarded_head(&head, &shared.token) {
        Ok(forwarded) => forwarded,
        Err(status) => {
            if status == 401 {
                (shared.log)(
                    "remote_gateway_request_refused",
                    serde_json::json!({ "reason": "unauthorized" }),
                );
            }
            reply(&mut socket, status).await;
            return;
        }
    };
    let request = shared.requests.fetch_add(1, Ordering::Relaxed) + 1;
    let started = Instant::now();
    let (mut send, mut recv) = match shared.client.open().await {
        Ok(stream) => stream,
        Err(failure) => {
            (shared.log)(
                "remote_gateway_request_failed",
                serde_json::json!({
                    "request": request,
                    "error_class": failure.class(),
                    "duration_ms": elapsed_ms(started),
                }),
            );
            if failure != Failure::Unreachable {
                (shared.on_failure)(failure);
            }
            reply(&mut socket, 502).await;
            return;
        }
    };
    let (mut local_read, mut local_write) = socket.split();
    let upstream = async {
        send.write_all(&forwarded).await.map_err(|_| ())?;
        send.write_all(&body_start).await.map_err(|_| ())?;
        let copied = tokio::io::copy(&mut local_read, &mut send).await;
        // The Frontend finished its request; the App Server still owes a response.
        let _ = send.finish();
        copied.map(|_| ()).map_err(|_| ())
    };
    let downstream = async {
        let copied = tokio::io::copy(&mut recv, &mut local_write).await;
        let _ = local_write.shutdown().await;
        copied
    };
    // The response side decides when a request is over, as on the App Server's
    // bridge: only a failed upload ends the request early.
    tokio::pin!(upstream, downstream);
    let outcome = tokio::select! {
        result = &mut downstream => result.map(|_| ()).map_err(|_| ()),
        result = &mut upstream => match result {
            Ok(()) => downstream.await.map(|_| ()).map_err(|_| ()),
            Err(()) => Err(()),
        },
    };
    // Healthy requests stay quiet: a session polls and reconnects continuously,
    // and a Frontend that abandons a request ends it the same way a fault does.
    // TODO: tell a lost connection from an abandoned request here and log only
    // the former, once the stream error kinds are needed for a diagnosis.
    let _ = outcome;
}

/// Checks the Frontend's token and returns the request head without it.
///
/// A preflight carries no credentials by design; the App Server answers it and
/// nothing else is reachable without the token. A Terminal socket (ADR-0071) is
/// the other exception: a browser WebSocket cannot send headers, and the App
/// Server admits it only with a single-use attach token issued over an
/// authorized request, so the gateway token would add nothing there.
fn forwarded_head(head: &[u8], token: &str) -> Result<Vec<u8>, u16> {
    let head = std::str::from_utf8(head).map_err(|_| 400u16)?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().filter(|line| !line.is_empty()).ok_or(400u16)?;
    let preflight = request_line.starts_with("OPTIONS ");
    let mut authorized = preflight || terminal_socket(request_line, head);
    let mut forwarded = Vec::with_capacity(head.len() + 4);
    forwarded.extend_from_slice(request_line.as_bytes());
    forwarded.extend_from_slice(b"\r\n");
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(400u16)?;
        // The App Server's bridge refuses such names; answer here instead.
        if name.is_empty() || name.contains(|character: char| character.is_ascii_whitespace()) {
            return Err(400);
        }
        if name.eq_ignore_ascii_case("authorization") {
            if value.trim().strip_prefix("Bearer ") != Some(token) {
                return Err(401);
            }
            authorized = true;
            continue;
        }
        forwarded.extend_from_slice(line.as_bytes());
        forwarded.extend_from_slice(b"\r\n");
    }
    if !authorized {
        return Err(401);
    }
    forwarded.extend_from_slice(b"\r\n");
    Ok(forwarded)
}

/// Exactly `GET /terminal` asking for an upgrade; the App Server checks the rest.
fn terminal_socket(request_line: &str, head: &str) -> bool {
    let target = request_line
        .strip_prefix("GET ")
        .and_then(|rest| rest.strip_suffix(" HTTP/1.1"));
    target.is_some_and(|target| target.split('?').next() == Some("/terminal"))
        && head.split("\r\n").skip(1).any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("upgrade")
                    && value.trim().eq_ignore_ascii_case("websocket")
            })
        })
}

async fn read_head(socket: &mut TcpStream) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = buffer
            .windows(HEAD_END.len())
            .position(|window| window == HEAD_END)
        {
            let body_start = buffer.split_off(end + HEAD_END.len());
            buffer.truncate(end);
            return Some((buffer, body_start));
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return None;
        }
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

async fn reply(socket: &mut TcpStream, status: u16) {
    let reason = match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        _ => "Bad Gateway",
    };
    // The Frontend runs on another origin; without this header it could not tell
    // a refusal from a dead port.
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.shutdown().await;
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
