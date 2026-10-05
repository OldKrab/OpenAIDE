//! WebSocket link for the reliable session. One socket carries both
//! directions: client frames are dispatched in order, server frames are pushed
//! as soon as the gateway signals a delivery.
//!
//! Wire messages are JSON text:
//!   client -> server  hello { transportVersion, connectionId, authToken?, sessionId?, receivedThrough }
//!   server -> client  ready { transportVersion, sessionId, serverId, receivedThrough }
//!   both              frame { sequence, message } | ack { through } | ping | pong
//! A definite server decision is reported as a close code (see `close_code`);
//! a socket that just disappears is the only resumable outcome.

use std::io::{BufReader, ErrorKind};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::logging;
use crate::protocol_edge::local_http::session_link::{LinkClose, SessionLink};
use crate::protocol_edge::local_http::{LocalHttpAppHandler, LocalHttpResponse};

use super::http::write_http_response;
use super::websocket::{
    write_close, write_handshake, write_pong, write_text, Incoming, MessageReader,
};
use super::{LocalHttpProbeListenerError, LocalHttpRequest};

const TRANSPORT_VERSION: u64 = 1;
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// Each side pings well inside this window, so silence means a dead peer.
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// A client whose timers are throttled still answers a ping from its message
/// handler, which keeps a hidden but healthy page inside the idle window.
const SERVER_PING_INTERVAL: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Bounds liveness renewal and time-driven deliveries when nothing signals.
const PUMP_FALLBACK_WAKE: Duration = Duration::from_millis(250);
/// Matches the largest client frame the chunked HTTP upload accepts.
const MAX_MESSAGE_BYTES: usize = 128 * 1024 * 1024;

const CLOSE_NORMAL: u16 = 1000;
const CLOSE_SUPERSEDED: u16 = 4000;
const CLOSE_PROTOCOL_VIOLATION: u16 = 4400;
const CLOSE_UNAUTHORIZED: u16 = 4401;
const CLOSE_FORBIDDEN: u16 = 4403;
const CLOSE_REPLAY_EXPIRED: u16 = 4409;
const CLOSE_SESSION_EXPIRED: u16 = 4410;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientMessage {
    #[serde(rename_all = "camelCase")]
    Hello {
        transport_version: u64,
        connection_id: String,
        auth_token: Option<String>,
        session_id: Option<String>,
        #[serde(default)]
        received_through: u64,
    },
    Frame {
        sequence: u64,
        message: Value,
    },
    Ack {
        through: u64,
    },
    Ping,
    Pong,
}

fn close_code(close: LinkClose) -> u16 {
    match close {
        LinkClose::Unauthorized => CLOSE_UNAUTHORIZED,
        LinkClose::Forbidden | LinkClose::WrongConnection => CLOSE_FORBIDDEN,
        LinkClose::SessionExpired => CLOSE_SESSION_EXPIRED,
        LinkClose::ReplayExpired => CLOSE_REPLAY_EXPIRED,
        LinkClose::Superseded => CLOSE_SUPERSEDED,
        LinkClose::InvalidConnectionId
        | LinkClose::InvalidAcknowledgement
        | LinkClose::SequenceGap
        | LinkClose::FrameRejected => CLOSE_PROTOCOL_VIOLATION,
    }
}

/// Serializes writes from the reader, dispatcher, and pump onto one socket and
/// records the first reason the socket ended.
struct Socket {
    writer: Mutex<TcpStream>,
    closed: AtomicBool,
    reason: Mutex<&'static str>,
    frames_received: AtomicU64,
    frames_sent: AtomicU64,
}

impl Socket {
    fn new(writer: TcpStream) -> Self {
        Self {
            writer: Mutex::new(writer),
            closed: AtomicBool::new(false),
            reason: Mutex::new("unknown"),
            frames_received: AtomicU64::new(0),
            frames_sent: AtomicU64::new(0),
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn send(&self, message: &Value) {
        if self.is_closed() {
            return;
        }
        let mut writer = self.writer.lock().expect("session socket writer poisoned");
        if write_text(&mut *writer, &message.to_string()).is_err() {
            drop(writer);
            self.end("write_failed", None);
        }
    }

    fn pong(&self, payload: &[u8]) {
        let mut writer = self.writer.lock().expect("session socket writer poisoned");
        if write_pong(&mut *writer, payload).is_err() {
            drop(writer);
            self.end("write_failed", None);
        }
    }

    fn close(&self, close: LinkClose) {
        self.end(close.reason_code(), Some(close_code(close)));
    }

    /// Ends the socket once. Shutting it down also releases the blocked reader.
    fn end(&self, reason: &'static str, code: Option<u16>) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        *self.reason.lock().expect("session socket reason poisoned") = reason;
        let mut writer = self.writer.lock().expect("session socket writer poisoned");
        if let Some(code) = code {
            let _ = write_close(&mut *writer, code, reason);
        }
        let _ = writer.shutdown(Shutdown::Both);
    }

    fn reason(&self) -> &'static str {
        *self.reason.lock().expect("session socket reason poisoned")
    }
}

pub(super) fn handle_session_socket(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> Result<(), LocalHttpProbeListenerError> {
    let Some(key) = request
        .websocket_key
        .as_deref()
        .filter(|_| request.websocket_version.as_deref() == Some("13"))
    else {
        logging::warn(
            "session_socket_rejected",
            json!({ "reason_code": "invalid_upgrade" }),
        );
        return write_http_response(
            stream,
            &LocalHttpResponse {
                status: 400,
                body: String::new(),
            },
        );
    };
    let started_at = Instant::now();
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(HELLO_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    // A conforming client sends nothing before the 101, so bytes the request
    // parser may have read past the headers are not carried over.
    write_handshake(stream, key)?;
    let socket = Socket::new(stream.try_clone()?);
    let mut reader = MessageReader::new(BufReader::new(stream.try_clone()?), MAX_MESSAGE_BYTES);

    let link = match open_link(&mut reader, handler, request.authorization.as_deref()) {
        Ok(link) => link,
        Err((reason, code)) => {
            logging::warn(
                "session_socket_rejected",
                json!({
                    "reason_code": reason,
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            );
            socket.end(reason, code);
            return Ok(());
        }
    };
    let (link, resumed, written_through) = link;
    logging::info(
        "session_socket_opened",
        json!({
            "connection_id": link.connection_id().as_str(),
            "session_id": link.session_id(),
            "resumed": resumed,
            "client_received_through": written_through,
            "server_received_through": link.client_received_through(),
        }),
    );
    socket.send(&json!({
        "type": "ready",
        "transportVersion": TRANSPORT_VERSION,
        "sessionId": link.session_id(),
        "serverId": link.server_id(),
        "receivedThrough": link.client_received_through(),
    }));
    stream.set_read_timeout(Some(READ_IDLE_TIMEOUT))?;

    let signal = link.delivery_signal();
    // Unbounded on purpose: a full queue would stall the reader, and with it
    // the pong replies the client uses to tell a busy server from a dead one.
    let (frames, queued_frames) = mpsc::channel::<(u64, Value)>();
    std::thread::scope(|scope| {
        scope.spawn(|| pump(&link, &socket, written_through));
        scope.spawn(|| dispatch(&link, handler, &socket, queued_frames));
        read(&mut reader, &link, &socket, frames);
        // The pump may be parked on the signal; let it observe the end now.
        signal.notify();
    });
    logging::info(
        "session_socket_closed",
        json!({
            "connection_id": link.connection_id().as_str(),
            "session_id": link.session_id(),
            "reason_code": socket.reason(),
            "frames_received": socket.frames_received.load(Ordering::Relaxed),
            "frames_sent": socket.frames_sent.load(Ordering::Relaxed),
            "duration_ms": started_at.elapsed().as_millis(),
        }),
    );
    Ok(())
}

/// Reads the hello and binds the link. Returns the link, whether it resumed a
/// session, and the client's cursor into the server sequence.
fn open_link(
    reader: &mut MessageReader<BufReader<TcpStream>>,
    handler: &LocalHttpAppHandler,
    upgrade_authorization: Option<&str>,
) -> Result<(SessionLink, bool, u64), (&'static str, Option<u16>)> {
    let hello = match reader.read() {
        Ok(Incoming::Text(text)) => serde_json::from_str::<ClientMessage>(&text).ok(),
        Ok(_) => None,
        Err(error) => return Err((read_failure_reason(&error), None)),
    };
    let Some(ClientMessage::Hello {
        transport_version,
        connection_id,
        auth_token,
        session_id,
        received_through,
    }) = hello
    else {
        return Err(("invalid_hello", Some(CLOSE_PROTOCOL_VIOLATION)));
    };
    if transport_version != TRANSPORT_VERSION {
        return Err((
            "unsupported_transport_version",
            Some(CLOSE_PROTOCOL_VIOLATION),
        ));
    }
    // A proxying shell authenticates the upgrade itself; a direct client has
    // no way to set headers on a browser WebSocket and sends the token here.
    let hello_authorization = auth_token.map(|token| format!("Bearer {token}"));
    let authorization = upgrade_authorization.or(hello_authorization.as_deref());
    let link = handler
        .open_session_link(
            authorization,
            Some(&connection_id),
            session_id.as_deref(),
            received_through,
        )
        .map_err(|close| (close.reason_code(), Some(close_code(close))))?;
    Ok((link, session_id.is_some(), received_through))
}

fn read(
    reader: &mut MessageReader<BufReader<TcpStream>>,
    link: &SessionLink,
    socket: &Socket,
    frames: mpsc::Sender<(u64, Value)>,
) {
    loop {
        let incoming = match reader.read() {
            Ok(incoming) => incoming,
            Err(error) => {
                let reason = read_failure_reason(&error);
                let code =
                    (error.kind() == ErrorKind::InvalidData).then_some(CLOSE_PROTOCOL_VIOLATION);
                socket.end(reason, code);
                return;
            }
        };
        let text = match incoming {
            Incoming::Text(text) => text,
            Incoming::Ping(payload) => {
                socket.pong(&payload);
                continue;
            }
            Incoming::Pong => continue,
            Incoming::Close => {
                socket.end("client_closed", Some(CLOSE_NORMAL));
                return;
            }
        };
        match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Frame { sequence, message }) => {
                if frames.send((sequence, message)).is_err() {
                    return;
                }
            }
            Ok(ClientMessage::Ack { through }) => {
                if let Err(close) = link.acknowledge(through) {
                    socket.close(close);
                    return;
                }
            }
            Ok(ClientMessage::Ping) => socket.send(&json!({ "type": "pong" })),
            Ok(ClientMessage::Pong) => {}
            Ok(ClientMessage::Hello { .. }) | Err(_) => {
                socket.end("invalid_message", Some(CLOSE_PROTOCOL_VIOLATION));
                return;
            }
        }
    }
}

/// Keeps client frames ordered. Runs apart from the reader so a slow RPC
/// cannot delay acknowledgements of the server sequence or pong replies.
fn dispatch(
    link: &SessionLink,
    handler: &LocalHttpAppHandler,
    socket: &Socket,
    queued_frames: Receiver<(u64, Value)>,
) {
    for (sequence, message) in queued_frames {
        if socket.is_closed() {
            return;
        }
        let accepted = match link.accept_frame(sequence, message) {
            Ok(accepted) => accepted,
            Err(close) => return socket.close(close),
        };
        // Acknowledged once accepted: the registry already suppresses a
        // resend, and the RPC's own outcome returns as a server frame.
        socket.send(&json!({ "type": "ack", "through": sequence }));
        let Some(accepted) = accepted else {
            continue;
        };
        socket.frames_received.fetch_add(1, Ordering::Relaxed);
        let outcome = link.dispatch_frame(accepted);
        handler.client_frame_handled();
        if let Err(close) = outcome {
            return socket.close(close);
        }
    }
}

/// Pushes server frames in sequence order. `written_through` only tracks what
/// this socket wrote; frames stay replayable until the client acknowledges.
fn pump(link: &SessionLink, socket: &Socket, mut written_through: u64) {
    let signal = link.delivery_signal();
    let mut last_ping = Instant::now();
    while !socket.is_closed() {
        if last_ping.elapsed() >= SERVER_PING_INTERVAL {
            socket.send(&json!({ "type": "ping" }));
            last_ping = Instant::now();
        }
        // Snapshot before draining so a delivery queued meanwhile still wakes.
        let seen = signal.generation();
        match link.pump(written_through) {
            Ok(frames) => {
                for frame in frames {
                    socket.send(&json!({
                        "type": "frame",
                        "sequence": frame.sequence,
                        "message": frame.message,
                    }));
                    if socket.is_closed() {
                        return;
                    }
                    written_through = frame.sequence;
                    socket.frames_sent.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(close) => return socket.close(close),
        }
        signal.wait_changed(seen, PUMP_FALLBACK_WAKE);
    }
}

fn read_failure_reason(error: &std::io::Error) -> &'static str {
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => "idle_timeout",
        ErrorKind::InvalidData => "protocol_violation",
        ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => {
            "peer_disconnected"
        }
        _ => "read_failed",
    }
}

#[cfg(test)]
#[path = "session_socket_tests.rs"]
mod tests;
