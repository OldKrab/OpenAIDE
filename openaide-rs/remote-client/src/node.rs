//! This device as a Remote Device (ADR-0062): its key, its endpoint, and the one
//! connection to the computer in use, with the rules a phone needs to keep it.
//!
//! One endpoint is in use at a time: two endpoints with the same key compete for
//! its relay. It is replaced, not kept for the life of the process, because a
//! phone's platform stops a hidden app's network and the endpoint cannot observe
//! that (see [`policy`]). A switch to another computer dials on the same endpoint
//! while the connection in use stays live, so the user stays where they are until
//! the other computer answers.
//!
//! The platform keeps what must outlive the process through [`NodeHost`] and says
//! when the app leaves and returns to the screen. Everything else is decided here.
// TODO: move the Desktop App and the Web App's gateway onto this node, so that
// [`RemoteClient`](crate::RemoteClient) and its own connect rules go away.

mod check;
mod dial;
mod policy;
mod receive_watch;
mod report;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, PublicKey, SecretKey};
use openaide_remote_core::{application_alpn, InviteCode, Network};

use crate::client::{bind_on, elapsed_ms, failure_of, redeem, Failure, PairedServer};
use crate::Logger;

pub use check::{CheckComputer, CheckRelay, CheckResult};
pub use report::{NetworkKind, Transport};

use policy::Timings;
use report::Purpose;

/// This device's own storage was unavailable; nothing was read or kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this device's storage is unavailable")]
pub struct StorageUnavailable;

/// What the platform keeps for a [`RemoteNode`], and what only it can observe.
/// Called from the node's tasks; each call returns promptly.
pub trait NodeHost: Send + Sync {
    /// This device's key, or `None` before its first bind.
    fn device_key(&self) -> Result<Option<SecretKey>, StorageUnavailable>;
    fn save_device_key(&self, key: &SecretKey) -> Result<(), StorageUnavailable>;
    /// The paired computer in use, which [`RemoteNode::connection`] reaches, or
    /// `None` when this device uses no paired computer.
    fn computer_in_use(&self) -> Option<PublicKey>;
    /// The relay `computer` last answered through, as kept by [`NodeHost::save_relay`].
    fn relay(&self, computer: &PublicKey) -> Option<String>;
    fn save_relay(&self, computer: &PublicKey, relay: &str) -> Result<(), StorageUnavailable>;
    /// How this device is online now, for the records of a dial and of a check.
    fn network(&self) -> NetworkKind {
        NetworkKind::UNKNOWN
    }
}

/// A handle to this device's node; clones share it.
#[derive(Clone)]
pub struct RemoteNode {
    inner: Arc<Inner>,
}

struct Inner {
    host: Arc<dyn NodeHost>,
    network: Network,
    alpn: Vec<u8>,
    timings: Timings,
    log: Logger,
    /// Runs what must finish even when its caller is cancelled.
    runtime: tokio::runtime::Handle,
    /// Held across a bind, so one endpoint is bound at a time.
    binding: tokio::sync::Mutex<Binding>,
    /// Held across a live connect, so concurrent requests share one.
    connecting: tokio::sync::Mutex<()>,
    live: Mutex<Live>,
    /// When the app left the screen; `None` while it is on screen.
    hidden_since: Mutex<Option<Instant>>,
    attempts: AtomicU32,
}

#[derive(Default)]
struct Binding {
    endpoint: Option<Endpoint>,
    binds: u32,
    /// Dials for a switch now on the endpoint; a retire waits for them.
    switch_dials: u32,
}

/// The connection in use and whether its endpoint must be replaced, under one lock
/// so no caller takes a connection that is being given up.
#[derive(Default)]
struct Live {
    connection: Option<Connection>,
    /// Why the endpoint must be replaced before its next use.
    stale: Option<&'static str>,
}

impl RemoteNode {
    /// A node on the public network. Call it within a Tokio runtime, which then
    /// runs the node's own tasks.
    pub fn new(host: Arc<dyn NodeHost>, log: Logger) -> Self {
        Self::on(Network::Public, host, log)
    }

    /// A node on `network`; tests pass a network of their own.
    pub fn on(network: Network, host: Arc<dyn NodeHost>, log: Logger) -> Self {
        Self::with_timings(network, host, log, Timings::default())
    }

    pub(crate) fn with_timings(
        network: Network,
        host: Arc<dyn NodeHost>,
        log: Logger,
        timings: Timings,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                host,
                network,
                alpn: application_alpn(),
                timings,
                log,
                runtime: tokio::runtime::Handle::current(),
                binding: tokio::sync::Mutex::default(),
                connecting: tokio::sync::Mutex::default(),
                live: Mutex::default(),
                hidden_since: Mutex::default(),
                attempts: AtomicU32::new(0),
            }),
        }
    }

    /// The app left the screen; the platform may stop its network from here on.
    // TODO: let the platform also report network changes through iroh's
    // `Endpoint::network_change`, which Android cannot observe from native code; it
    // may make replacing the endpoint on return unnecessary. Measure before
    // changing the rule.
    pub fn hidden(&self) {
        *lock(&self.inner.hidden_since) = Some(Instant::now());
    }

    /// The app is on screen again. After more than a short pause the endpoint is
    /// replaced before the next dial, and a dial already waiting on it is ended; a
    /// switch's dial instead runs out its attempt, and the retire follows it.
    pub fn shown(&self) {
        let Some(since) = lock(&self.inner.hidden_since).take() else {
            return;
        };
        if !self.inner.timings.stale_after_hidden(since.elapsed()) {
            return;
        }
        self.mark_stale("resumed");
        let node = self.clone();
        self.inner.runtime.spawn(async move {
            let mut binding = node.inner.binding.lock().await;
            node.retire(&mut binding);
        });
    }

    /// Spends an invite: connects to the App Server it names and presents its secret.
    pub async fn redeem(
        &self,
        invite: &InviteCode,
        name: &str,
        model: Option<&str>,
    ) -> Result<PairedServer, Failure> {
        let endpoint = self.endpoint().await?;
        redeem(&endpoint, invite, name, model, &self.inner.log).await
    }

    /// The live connection to the computer in use, opened or reopened on demand.
    /// One slow or stalled dial is not yet an unreachable computer: each further
    /// attempt starts on a new endpoint while the caller keeps showing "Connecting…".
    pub async fn connection(&self) -> Result<Connection, Failure> {
        if let Some(connection) = self.live() {
            return Ok(connection);
        }
        let _connecting = self.inner.connecting.lock().await;
        if let Some(connection) = self.live() {
            return Ok(connection);
        }
        let server = self
            .inner
            .host
            .computer_in_use()
            .ok_or(Failure::NotTrusted)?;
        let connection = self.reach(server, Purpose::Live).await?;
        self.use_connection(connection.clone());
        Ok(connection)
    }

    /// Opens the stream for one request, replacing a connection that died since
    /// the last request once.
    pub async fn open(&self) -> Result<(SendStream, RecvStream), Failure> {
        let mut failure = Failure::Unreachable;
        for _ in 0..2 {
            let connection = self.connection().await?;
            match connection.open_bi().await {
                Ok(stream) => return Ok(stream),
                Err(_) => failure = self.drop_connection(&connection),
            }
            if failure != Failure::Unreachable {
                break;
            }
        }
        Err(failure)
    }

    /// Reaches `server` for a switch while the connection in use stays live: the
    /// result is not used until [`RemoteNode::adopt`] takes it, and a failed attempt
    /// keeps the endpoint that connection rides on.
    pub async fn reach_for_switch(&self, server: PublicKey) -> Result<Connection, Failure> {
        self.reach(server, Purpose::Switch).await
    }

    /// Makes a connection reached for a switch the one in use, ending the previous one.
    pub fn adopt(&self, next: Connection) {
        let previous = lock(&self.inner.live).connection.replace(next.clone());
        if let Some(previous) = previous.filter(|previous| !same(previous, &next)) {
            previous.close(0u32.into(), b"switched");
        }
        self.log(
            "remote_connection_adopted",
            serde_json::json!({ "attempt": self.attempts() }),
        );
        self.watch_receive(next, self.attempts());
    }

    /// Ends a connection reached for a switch that was cancelled or could not be saved.
    pub fn abandon(&self, unused: &Connection) {
        if self.is_in_use(unused) {
            return;
        }
        unused.close(0u32.into(), b"cancelled");
        self.log(
            "remote_connection_abandoned",
            serde_json::json!({ "attempt": self.attempts() }),
        );
    }

    /// Forgets a connection a stream could not be opened on; returns why it ended.
    pub fn drop_connection(&self, failed: &Connection) -> Failure {
        {
            let mut live = lock(&self.inner.live);
            if live
                .connection
                .as_ref()
                .is_some_and(|current| same(current, failed))
            {
                live.connection = None;
            }
        }
        failed
            .close_reason()
            .map_or(Failure::Unreachable, |reason| failure_of(&reason))
    }

    /// Ends the connection when the user forgets the computer or uses another.
    pub fn disconnect(&self) {
        let Some(current) = lock(&self.inner.live).connection.take() else {
            return;
        };
        current.close(0u32.into(), b"forgotten");
        self.log(
            "remote_disconnected",
            serde_json::json!({ "attempt": self.attempts() }),
        );
    }

    /// Binds the endpoint with this device's stored key, creating the key on first use.
    async fn endpoint(&self) -> Result<Endpoint, Failure> {
        let mut binding = self.inner.binding.lock().await;
        self.bound_endpoint(&mut binding).await
    }

    /// The endpoint for one dial of a switch, counted until [`RemoteNode::end_switch_dial`]
    /// so a reconnect of the live connection meanwhile cannot retire it from under the dial.
    async fn switch_dial_endpoint(&self) -> Result<Endpoint, Failure> {
        let mut binding = self.inner.binding.lock().await;
        let endpoint = self.bound_endpoint(&mut binding).await?;
        binding.switch_dials += 1;
        Ok(endpoint)
    }

    async fn end_switch_dial(&self, succeeded: bool) {
        let mut binding = self.inner.binding.lock().await;
        binding.switch_dials = binding.switch_dials.saturating_sub(1);
        if binding.switch_dials > 0 {
            return;
        }
        let kept = {
            let mut live = lock(&self.inner.live);
            // The dial just reached a computer on this endpoint, so whatever marked it
            // stale meanwhile was wrong about its network; retiring it would end the
            // new connection.
            if succeeded {
                live.stale.take()
            } else {
                None
            }
        };
        match kept {
            Some(reason) => self.log(
                "remote_endpoint_kept",
                serde_json::json!({ "bind": binding.binds, "reason": reason }),
            ),
            None => self.retire(&mut binding),
        }
    }

    async fn bound_endpoint(&self, binding: &mut Binding) -> Result<Endpoint, Failure> {
        self.retire(binding);
        if let Some(endpoint) = &binding.endpoint {
            return Ok(endpoint.clone());
        }
        binding.binds += 1;
        let bind = binding.binds;
        let started = Instant::now();
        self.log(
            "remote_endpoint_bind_started",
            serde_json::json!({ "bind": bind }),
        );
        let result = self.bind().await;
        let outcome = match &result {
            Ok(_) => "ready",
            Err(Failure::Storage) => "storage_unavailable",
            Err(_) => "failed",
        };
        self.log(
            "remote_endpoint_bind_completed",
            serde_json::json!({ "bind": bind, "outcome": outcome, "duration_ms": elapsed_ms(started) }),
        );
        let endpoint = result?;
        binding.endpoint = Some(endpoint.clone());
        Ok(endpoint)
    }

    async fn bind(&self) -> Result<Endpoint, Failure> {
        let host = &self.inner.host;
        let key = match host.device_key().map_err(|_| Failure::Storage)? {
            Some(key) => key,
            None => {
                let key = crate::new_device_key();
                host.save_device_key(&key).map_err(|_| Failure::Storage)?;
                key
            }
        };
        bind_on(&self.inner.network, key, self.inner.log.clone()).await
    }

    /// Gives up the endpoint marked stale and its connection, unless a switch is
    /// dialling on it. Closing can take seconds on a dead network, so it does not
    /// delay the endpoint that follows.
    fn retire(&self, binding: &mut Binding) {
        let reason = {
            let mut live = lock(&self.inner.live);
            let Some(reason) = live.stale else { return };
            if policy::retire_now(binding.switch_dials) {
                live.connection = None;
                live.stale = None;
                Some(reason)
            } else {
                // The switch's dial ends on its own; its end or the next bind retires.
                self.log(
                    "remote_endpoint_retire_deferred",
                    serde_json::json!({ "bind": binding.binds, "reason": reason }),
                );
                None
            }
        };
        let (Some(reason), Some(old)) = (reason, binding.endpoint.take()) else {
            return;
        };
        let bind = binding.binds;
        self.log(
            "remote_endpoint_retired",
            serde_json::json!({ "bind": bind, "reason": reason }),
        );
        let log = self.inner.log.clone();
        self.inner.runtime.spawn(async move {
            let started = Instant::now();
            old.close().await;
            log(
                "remote_endpoint_closed",
                serde_json::json!({ "bind": bind, "duration_ms": elapsed_ms(started) }),
            );
        });
    }

    /// Makes `opened` the live connection; its receive watch lasts while it stays live.
    fn use_connection(&self, opened: Connection) {
        lock(&self.inner.live).connection = Some(opened.clone());
        self.watch_receive(opened, self.attempts());
    }

    /// The open connection, unless its endpoint is about to be replaced.
    fn live(&self) -> Option<Connection> {
        let live = lock(&self.inner.live);
        if live.stale.is_some() {
            return None;
        }
        live.connection
            .clone()
            .filter(|connection| connection.close_reason().is_none())
    }

    fn is_in_use(&self, connection: &Connection) -> bool {
        lock(&self.inner.live)
            .connection
            .as_ref()
            .is_some_and(|current| same(current, connection))
    }

    fn mark_stale(&self, reason: &'static str) {
        lock(&self.inner.live).stale = Some(reason);
    }

    fn is_hidden(&self) -> bool {
        lock(&self.inner.hidden_since).is_some()
    }

    fn attempts(&self) -> u32 {
        self.inner.attempts.load(Ordering::Relaxed)
    }

    fn log(&self, event: &'static str, fields: serde_json::Value) {
        (self.inner.log)(event, fields);
    }
}

/// Whether two handles are one connection. The id is the connection's address in
/// memory, which stays unique while both handles hold it.
fn same(left: &Connection, right: &Connection) -> bool {
    left.stable_id() == right.stable_id()
}

/// The node's state stays usable after a panic elsewhere; each lock guards plain values.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[cfg(test)]
#[path = "node/tests.rs"]
mod tests;
