//! The launcher's [`SshTransport`] over the embedded client.
//!
//! Commands run on the shared session. The forward listens on this phone's
//! loopback and opens one `direct-tcpip` channel per accepted connection, so a
//! connection made after the session dropped opens a new session first. The
//! gateway sends each WebView request as its own connection, which makes that
//! the reconnect: the next request after a lost network finds a fresh session.

use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use openaide_remote_client::{RemoteOutput, SshTransport, TransportError};
use russh::ChannelMsg;
use serde_json::json;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::session::Connector;
use crate::Log;

/// Bounds what a launcher command may print; its scripts print a few lines.
const MAX_STDOUT_BYTES: usize = 1024 * 1024;
const UPLOAD_CHUNK_BYTES: usize = 32 * 1024;

pub(crate) struct EmbeddedSsh {
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) connector: Arc<Connector>,
    pub(crate) log: Log,
}

/// Ends the forward's listener when dropped; channels in flight finish.
pub struct Forward {
    listener: JoinHandle<()>,
}

impl Drop for Forward {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

impl SshTransport for EmbeddedSsh {
    type Forward = Forward;

    fn run(
        &mut self,
        command: &str,
        input: Option<&mut dyn Read>,
    ) -> Result<RemoteOutput, TransportError> {
        let connector = self.connector.clone();
        self.runtime
            .block_on(run(&connector, command, input))
            .map_err(|class| TransportError { class })
    }

    fn forward(&mut self, remote_port: u16) -> Result<(Forward, SocketAddr), TransportError> {
        let listener = self
            .runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .map_err(|_| TransportError {
                class: "local_port_unavailable",
            })?;
        let local = listener.local_addr().map_err(|_| TransportError {
            class: "local_port_unavailable",
        })?;
        let connector = self.connector.clone();
        let log = self.log.clone();
        let accept = self
            .runtime
            .spawn(accept(listener, connector, log, remote_port));
        Ok((Forward { listener: accept }, local))
    }
}

async fn run(
    connector: &Connector,
    command: &str,
    input: Option<&mut dyn Read>,
) -> Result<RemoteOutput, &'static str> {
    let session = connector.session().await?;
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|error| crate::session::classify(&error))?;
    let lost = |error: russh::Error| crate::session::classify(&error);
    channel.exec(true, command).await.map_err(lost)?;
    if let Some(input) = input {
        // The launcher's input is a local file; reading it here blocks only
        // the caller's thread, which is waiting for this command anyway.
        let mut buffer = vec![0; UPLOAD_CHUNK_BYTES];
        loop {
            let read = input.read(&mut buffer).map_err(|_| "release_unreadable")?;
            if read == 0 {
                break;
            }
            channel.data(&buffer[..read]).await.map_err(lost)?;
        }
    }
    channel.eof().await.map_err(lost)?;
    let mut stdout = Vec::new();
    let mut status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if stdout.len() + data.len() > MAX_STDOUT_BYTES {
                    return Err("output_too_large");
                }
                stdout.extend_from_slice(&data);
            }
            ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            ChannelMsg::Failure => return Err("command_refused"),
            _ => {}
        }
    }
    // A channel that closes with no exit status means the session ended under it.
    let status = status.ok_or("connection_lost")?;
    Ok(RemoteOutput {
        success: status == 0,
        stdout,
    })
}

async fn accept(
    listener: tokio::net::TcpListener,
    connector: Arc<Connector>,
    log: Log,
    remote_port: u16,
) {
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            log("ssh_forward_accept_failed", json!({}));
            continue;
        };
        let connector = connector.clone();
        let log = log.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            if let Err(class) = carry(stream, peer, &connector, remote_port).await {
                // A healthy forward is quiet; only failures are logged.
                log(
                    "ssh_forward_channel_failed",
                    json!({
                        "errorClass": class,
                        "durationMs": started.elapsed().as_millis() as u64,
                    }),
                );
            }
        });
    }
}

async fn carry(
    mut stream: tokio::net::TcpStream,
    peer: SocketAddr,
    connector: &Connector,
    remote_port: u16,
) -> Result<(), &'static str> {
    let open = |session: Arc<russh::client::Handle<crate::session::PinnedHost>>| async move {
        session
            .channel_open_direct_tcpip(
                "127.0.0.1",
                u32::from(remote_port),
                "127.0.0.1",
                u32::from(peer.port()),
            )
            .await
    };
    let session = connector.session().await?;
    let channel = match open(session.clone()).await {
        Ok(channel) => channel,
        // The session ended under this request; the next session is new.
        Err(_) if session.is_closed() => open(connector.session().await?)
            .await
            .map_err(|error| crate::session::classify(&error))?,
        // The App Server's port refused the channel, such as after it stopped.
        Err(error) => return Err(crate::session::classify(&error)),
    };
    let mut remote = channel.into_stream();
    tokio::io::copy_bidirectional(&mut stream, &mut remote)
        .await
        .map(|_| ())
        .map_err(|_| "connection_lost")
}
