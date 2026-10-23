//! WebSocket carrying one Terminal's bytes (ADR-0071). Lifecycle stays on the App Server
//! Protocol; this socket only streams, so keystrokes never queue behind other traffic.
//!
//! Wire messages:
//!   client -> server  text hello { token, cols, rows }, then binary input, text resize { cols, rows },
//!                     and text pong answering each ping
//!   server -> client  text ready { terminalId }, then binary output: the replay first, then live,
//!                     with a text ping every `SERVER_PING_INTERVAL`
//! The token comes from `terminals/open` or `terminals/attach` and is redeemed once. Close codes
//! tell the client what to do next (see the constants).

use std::io::{BufReader, ErrorKind};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;

use crate::logging;
use crate::protocol_edge::local_http::{LocalHttpAppHandler, LocalHttpResponse};
use crate::terminals::{valid_size, Shell};

use super::http::write_http_response;
use super::websocket::{
    write_binary, write_close, write_handshake, write_pong, write_text, Incoming, MessageReader,
};
use super::{LocalHttpProbeListenerError, LocalHttpRequest};

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// Three missed pongs. The client applies the same budget to the server's pings.
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(45);
/// Pings are text, not WebSocket control frames, because a browser answers those itself and
/// never shows them to the page, which then could not notice a server that went silent. They
/// run on a timer, not on idle output, so a viewer that only watches a busy shell still pongs.
#[cfg(not(test))]
const SERVER_PING_INTERVAL: Duration = Duration::from_secs(15);
#[cfg(test)]
const SERVER_PING_INTERVAL: Duration = Duration::from_millis(200);
const PING: &str = r#"{"type":"ping"}"#;
/// A viewer that cannot take output this long is gone or too slow; it reattaches.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Larger pastes arrive as several messages.
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
/// Keeps one replay frame from monopolizing a relayed path.
const REPLAY_FRAME_BYTES: usize = 64 * 1024;

/// The shell ended or the Terminal closed; the published status says which.
const CLOSE_TERMINAL_ENDED: u16 = 1000;
const CLOSE_PROTOCOL_VIOLATION: u16 = 4400;
/// The token is unknown, used, or expired; ask `terminals/attach` for another.
const CLOSE_UNAUTHORIZED: u16 = 4401;
/// This viewer fell behind the output and was dropped; reattach to resume from the replay.
const CLOSE_VIEWER_BEHIND: u16 = 4408;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientMessage {
    Hello { token: String, cols: u16, rows: u16 },
    Resize { cols: u16, rows: u16 },
    Pong,
}

/// Serializes writes from the reader and the output pump, and records the first end reason.
struct Socket {
    writer: Mutex<TcpStream>,
    closed: AtomicBool,
    reason: Mutex<&'static str>,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
}

impl Socket {
    fn write(&self, write: impl FnOnce(&mut TcpStream) -> std::io::Result<()>) -> bool {
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        let mut writer = self.writer.lock().expect("terminal socket writer poisoned");
        if write(&mut writer).is_err() {
            drop(writer);
            self.end("write_failed", None);
            return false;
        }
        true
    }

    /// Ends the socket once. Shutting it down also releases the blocked reader.
    fn end(&self, reason: &'static str, code: Option<u16>) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        *self.reason.lock().expect("terminal socket reason poisoned") = reason;
        let mut writer = self.writer.lock().expect("terminal socket writer poisoned");
        if let Some(code) = code {
            let _ = write_close(&mut *writer, code, reason);
        }
        let _ = writer.shutdown(Shutdown::Both);
    }

    fn reason(&self) -> &'static str {
        *self.reason.lock().expect("terminal socket reason poisoned")
    }
}

pub(super) fn handle_terminal_socket(
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
            "terminal_socket_rejected",
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
    write_handshake(stream, key)?;
    let socket = Socket {
        writer: Mutex::new(stream.try_clone()?),
        closed: AtomicBool::new(false),
        reason: Mutex::new("unknown"),
        bytes_in: AtomicU64::new(0),
        bytes_out: AtomicU64::new(0),
    };
    let mut reader = MessageReader::new(BufReader::new(stream.try_clone()?), MAX_MESSAGE_BYTES);

    let shell = match read_hello(&mut reader, handler) {
        Ok(shell) => shell,
        Err((reason, code)) => {
            logging::warn(
                "terminal_socket_rejected",
                json!({ "reason_code": reason, "duration_ms": started_at.elapsed().as_millis() }),
            );
            socket.end(reason, code);
            return Ok(());
        }
    };
    let (viewer_id, attach) = shell.attach();
    logging::info(
        "terminal_socket_opened",
        json!({
            "terminal_id": shell.terminal_id,
            "viewer_id": viewer_id,
            "replay_bytes": attach.replay.len(),
            "viewers": shell.viewer_count(),
        }),
    );
    socket.write(|writer| {
        write_text(
            writer,
            &json!({ "type": "ready", "terminalId": shell.terminal_id }).to_string(),
        )
    });
    for chunk in attach.replay.chunks(REPLAY_FRAME_BYTES) {
        if !socket.write(|writer| write_binary(writer, chunk)) {
            break;
        }
        socket
            .bytes_out
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    stream.set_read_timeout(Some(READ_IDLE_TIMEOUT))?;

    std::thread::scope(|scope| {
        scope.spawn(|| pump(&shell, &socket, attach.output));
        read(&mut reader, &shell, &socket);
        // Dropping the viewer's channel wakes the pump now instead of at its next ping.
        shell.detach(viewer_id);
    });
    logging::info(
        "terminal_socket_closed",
        json!({
            "terminal_id": shell.terminal_id,
            "viewer_id": viewer_id,
            "reason_code": socket.reason(),
            "bytes_in": socket.bytes_in.load(Ordering::Relaxed),
            "bytes_out": socket.bytes_out.load(Ordering::Relaxed),
            "duration_ms": started_at.elapsed().as_millis(),
        }),
    );
    Ok(())
}

fn read_hello(
    reader: &mut MessageReader<BufReader<TcpStream>>,
    handler: &LocalHttpAppHandler,
) -> Result<Arc<Shell>, (&'static str, Option<u16>)> {
    let hello = match reader.read() {
        Ok(Incoming::Text(text)) => serde_json::from_str::<ClientMessage>(&text).ok(),
        Ok(_) => None,
        Err(error) => return Err((read_failure_reason(&error), None)),
    };
    let Some(ClientMessage::Hello { token, cols, rows }) = hello else {
        return Err(("invalid_hello", Some(CLOSE_PROTOCOL_VIOLATION)));
    };
    let shell = handler
        .terminals()
        .claim(&token)
        .ok_or(("invalid_token", Some(CLOSE_UNAUTHORIZED)))?;
    // The most recently attached client sets the size, as it does with each later resize.
    if valid_size(cols, rows) {
        shell.resize(cols, rows);
    }
    Ok(shell)
}

/// Forwards live output and pings on a fixed schedule, busy shell or quiet.
fn pump(shell: &Shell, socket: &Socket, output: std::sync::mpsc::Receiver<Vec<u8>>) {
    let mut next_ping = Instant::now() + SERVER_PING_INTERVAL;
    loop {
        if Instant::now() >= next_ping {
            if !socket.write(|writer| write_text(writer, PING)) {
                return;
            }
            next_ping = Instant::now() + SERVER_PING_INTERVAL;
        }
        match output.recv_timeout(next_ping.saturating_duration_since(Instant::now())) {
            Ok(chunk) => {
                if !socket.write(|writer| write_binary(writer, &chunk)) {
                    return;
                }
                socket
                    .bytes_out
                    .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if shell.output_ended() {
                    socket.end("terminal_ended", Some(CLOSE_TERMINAL_ENDED));
                } else {
                    socket.end("viewer_behind", Some(CLOSE_VIEWER_BEHIND));
                }
                return;
            }
        }
    }
}

fn read(reader: &mut MessageReader<BufReader<TcpStream>>, shell: &Shell, socket: &Socket) {
    loop {
        let incoming = match reader.read() {
            Ok(incoming) => incoming,
            Err(error) => {
                let code =
                    (error.kind() == ErrorKind::InvalidData).then_some(CLOSE_PROTOCOL_VIOLATION);
                socket.end(read_failure_reason(&error), code);
                return;
            }
        };
        match incoming {
            Incoming::Binary(bytes) => {
                socket
                    .bytes_in
                    .fetch_add(bytes.len() as u64, Ordering::Relaxed);
                if !bytes.is_empty() && !shell.write_input(bytes) {
                    socket.end("terminal_ended", Some(CLOSE_TERMINAL_ENDED));
                    return;
                }
            }
            Incoming::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Pong) => {}
                Ok(ClientMessage::Resize { cols, rows }) if valid_size(cols, rows) => {
                    if !shell.resize(cols, rows) {
                        logging::warn(
                            "terminal_resize_failed",
                            json!({ "terminal_id": shell.terminal_id }),
                        );
                    }
                }
                _ => {
                    socket.end("invalid_message", Some(CLOSE_PROTOCOL_VIOLATION));
                    return;
                }
            },
            Incoming::Ping(payload) => {
                socket.write(|writer| write_pong(writer, &payload));
            }
            Incoming::Pong => {}
            Incoming::Close => {
                socket.end("client_closed", Some(1000));
                return;
            }
        }
        if socket.closed.load(Ordering::Acquire) {
            return;
        }
    }
}

fn read_failure_reason(error: &std::io::Error) -> &'static str {
    match error.kind() {
        ErrorKind::WouldBlock | ErrorKind::TimedOut => "read_timeout",
        ErrorKind::UnexpectedEof => "peer_closed",
        ErrorKind::InvalidData => "protocol_violation",
        _ => "read_failed",
    }
}

#[cfg(all(test, unix))]
#[path = "terminal_socket_tests.rs"]
mod tests;
