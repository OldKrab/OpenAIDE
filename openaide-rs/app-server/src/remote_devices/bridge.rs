//! Carries one Remote Device stream to the App Server's own request handlers.
//!
//! A Remote Device opens one stream per request and speaks the same HTTP the
//! same-machine App Shells do. Its identity was already proven by the
//! connection's key, so the bridge replaces whatever credentials the request
//! carries with the local token and forwards the bytes unchanged. The device
//! never learns that token.
//
// TODO: make the request handlers generic over the byte stream instead of
// `TcpStream`, then call them directly with a pre-authenticated identity. That
// removes this loopback hop and the header rewrite, and lets a handler know
// which device it serves without the connection-id side table.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Where a trusted stream is delivered: the local listener and the token it accepts.
#[derive(Clone)]
pub(crate) struct BridgeTarget {
    pub address: SocketAddr,
    pub auth_token: String,
}

const MAX_HEAD_BYTES: usize = 16 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(30);
const HEAD_END: &[u8] = b"\r\n\r\n";
const CONNECTION_ID_HEADER: &str = "x-openaide-connection-id";

#[derive(Debug, thiserror::Error)]
pub(crate) enum BridgeError {
    #[error("the request head did not arrive in time")]
    HeadTimeout,
    #[error("the request head is malformed or too large")]
    InvalidHead,
    #[error("the stream closed")]
    Stream,
    #[error("the local listener is unavailable")]
    Local,
}

impl BridgeError {
    pub(crate) fn class(&self) -> &'static str {
        match self {
            Self::HeadTimeout => "head_timeout",
            Self::InvalidHead => "invalid_head",
            Self::Stream => "stream_closed",
            Self::Local => "local_unavailable",
        }
    }
}

/// The rewritten request head and the facts the edge records about it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TrustedHead {
    pub bytes: Vec<u8>,
    pub connection_id: Option<String>,
}

/// Runs until either side finishes. `on_head` sees the protocol connection id once.
pub(crate) async fn forward<R, W>(
    mut recv: R,
    mut send: W,
    target: &BridgeTarget,
    on_head: impl FnOnce(&TrustedHead),
) -> Result<(), BridgeError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let (head, body_start) = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut recv))
        .await
        .map_err(|_| BridgeError::HeadTimeout)??;
    let trusted = trust_head(&head, &target.auth_token)?;
    on_head(&trusted);

    let mut local = TcpStream::connect(target.address)
        .await
        .map_err(|_| BridgeError::Local)?;
    local
        .write_all(&trusted.bytes)
        .await
        .map_err(|_| BridgeError::Local)?;
    local
        .write_all(&body_start)
        .await
        .map_err(|_| BridgeError::Local)?;

    let (mut local_read, mut local_write) = local.split();
    let upstream = async {
        let copied = tokio::io::copy(&mut recv, &mut local_write).await;
        // The device finished its request; the handler still owes a response.
        let _ = local_write.shutdown().await;
        copied
    };
    let downstream = async {
        let copied = tokio::io::copy(&mut local_read, &mut send).await;
        let _ = send.shutdown().await;
        copied
    };
    // The response side decides when a request is over: a handler closes its
    // socket after answering. A device that finished sending still awaits that
    // answer, so only a failed upload ends the request early.
    tokio::pin!(upstream, downstream);
    tokio::select! {
        result = &mut downstream => result.map(|_| ()).map_err(|_| BridgeError::Stream),
        result = &mut upstream => match result {
            Ok(_) => downstream.await.map(|_| ()).map_err(|_| BridgeError::Stream),
            Err(_) => Err(BridgeError::Stream),
        },
    }
}

async fn read_head<R>(recv: &mut R) -> Result<(Vec<u8>, Vec<u8>), BridgeError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = find(&buffer, HEAD_END) {
            let body_start = buffer.split_off(end + HEAD_END.len());
            return Ok((buffer, body_start));
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Err(BridgeError::InvalidHead);
        }
        let read = recv
            .read(&mut chunk)
            .await
            .map_err(|_| BridgeError::Stream)?;
        if read == 0 {
            return Err(BridgeError::Stream);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// Drops every credential the device sent and adds the one the local listener accepts.
///
/// The local credential goes first because the listener reads the first header
/// of a name, and it tolerates bare line feeds this rewrite refuses.
pub(crate) fn trust_head(head: &[u8], auth_token: &str) -> Result<TrustedHead, BridgeError> {
    let head = std::str::from_utf8(head).map_err(|_| BridgeError::InvalidHead)?;
    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .filter(|line| !line.is_empty() && is_single_line(line))
        .ok_or(BridgeError::InvalidHead)?;
    let mut bytes = Vec::with_capacity(head.len() + auth_token.len() + 32);
    bytes.extend_from_slice(request_line.as_bytes());
    bytes.extend_from_slice(format!("\r\nAuthorization: Bearer {auth_token}\r\n").as_bytes());
    let mut connection_id = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(BridgeError::InvalidHead)?;
        // Whitespace in a name or a stray line break is how a second credential is
        // smuggled past a filter that the receiving parser then normalizes.
        if name.is_empty()
            || name.contains(|character: char| character.is_ascii_whitespace())
            || !is_single_line(line)
        {
            return Err(BridgeError::InvalidHead);
        }
        if name.eq_ignore_ascii_case("authorization") {
            continue;
        }
        if name.eq_ignore_ascii_case(CONNECTION_ID_HEADER) {
            connection_id = Some(value.trim().to_string());
        }
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\r\n");
    }
    bytes.extend_from_slice(b"\r\n");
    Ok(TrustedHead {
        bytes,
        connection_id,
    })
}

fn is_single_line(line: &str) -> bool {
    !line.contains(['\r', '\n'])
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
