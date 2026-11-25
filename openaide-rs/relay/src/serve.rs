//! The running relay: HTTPS on one TCP port, QUIC address discovery on one UDP port, the
//! certificate keeper, and the key file check.
//!
//! iroh's ready-made relay server cannot take extra HTTP routes and does not expose its
//! connected clients, so this module accepts TCP itself and hands each connection to
//! iroh's [`RelayService`], which carries the management API routes and this relay's
//! access check. QUIC address discovery is private to iroh, so it runs as an iroh server
//! with only QUIC enabled, sharing the same certificate resolver.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use http::{header, HeaderMap, Method, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;
use iroh_relay::http::RELAY_PROBE_PATH;
use iroh_relay::server::http_server::{BytesBody, RelayService, TlsConfig};
use iroh_relay::server::{Handlers, Metrics, QuicConfig};
use iroh_relay::KeyCache;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

use crate::access::RelayAccess;
use crate::acme::{self, AcmeSettings};
use crate::api::ManagementApi;
use crate::config::{CertSource, Host, RelayConfig};
use crate::keys::KeyStore;
use crate::log;
use crate::tls::{self, CertResolver};

/// How long a new connection has to finish its TLS and relay handshakes.
const ESTABLISH_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the key file is checked for keys revoked by a command.
pub const KEY_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// A relay for a few people needs only a small cache of endpoint keys.
const KEY_CACHE_CAPACITY: usize = 4096;
/// After a failed accept (such as running out of file handles), wait before the next one.
const ACCEPT_RETRY_PAUSE: Duration = Duration::from_millis(100);

const INDEX_TEXT: &str = "This is an OpenAIDE relay.\n\
To use it, ask the person who runs it for a relay key, then paste the key into \
OpenAIDE under Settings, Remote, Relay.\n";

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub state_dir: PathBuf,
    /// Where HTTPS listens.
    pub https_bind: SocketAddr,
    /// Where QUIC address discovery listens; `None` turns it off.
    pub quic_bind: Option<SocketAddr>,
    /// The public address that key lines carry.
    pub host: Host,
    /// The public HTTPS port; `None` means the port HTTPS is bound to.
    pub public_port: Option<u16>,
    pub cert: CertSource,
    pub acme_directory: String,
}

impl ServeOptions {
    /// The options `setup` configured, listening on every address.
    pub fn from_config(state_dir: PathBuf, config: &RelayConfig) -> Self {
        Self {
            state_dir,
            https_bind: (Ipv6Addr::UNSPECIFIED, config.port).into(),
            quic_bind: config
                .quic_port
                .map(|port| (Ipv6Addr::UNSPECIFIED, port).into()),
            host: config.host.clone(),
            public_port: Some(config.port),
            cert: config.cert.clone(),
            acme_directory: config
                .acme_directory
                .clone()
                .unwrap_or_else(|| acme::LETS_ENCRYPT_PRODUCTION.to_string()),
        }
    }
}

/// A started relay. Dropping it stops every task it started.
#[derive(Debug)]
pub struct RunningRelay {
    https_addr: SocketAddr,
    quic_addr: Option<SocketAddr>,
    service: RelayService,
    access: Arc<RelayAccess>,
    store: Arc<KeyStore>,
    resolver: Arc<CertResolver>,
    quic: Option<iroh_relay::server::Server>,
    tasks: JoinSet<()>,
}

impl RunningRelay {
    pub async fn start(options: ServeOptions) -> std::io::Result<Self> {
        let store = Arc::new(KeyStore::open(&options.state_dir));
        let access = Arc::new(RelayAccess::new(store.clone()));
        let resolver = CertResolver::new();
        let server_config = Arc::new(tls::server_config(resolver.clone())?);
        let mut tasks = JoinSet::new();

        match &options.cert {
            CertSource::Files { dir } => {
                let mut watcher = tls::CertDirWatcher::new(dir.clone(), resolver.clone());
                // The first load happens before the port opens, so the first handshake
                // already has a certificate.
                watcher.check();
                tasks.spawn(watcher.run());
            }
            CertSource::Acme => {
                let settings = AcmeSettings {
                    host: options.host.clone(),
                    directory: options.acme_directory.clone(),
                    state_dir: options.state_dir.clone(),
                };
                tasks.spawn(acme::run(settings, resolver.clone()));
            }
        }

        let listener = bind_tcp(options.https_bind).await?;
        let https_addr = listener.local_addr()?;
        let api = Arc::new(ManagementApi::new(
            store.clone(),
            access.clone(),
            options.host.clone(),
            options.public_port.unwrap_or(https_addr.port()),
        ));
        let mut handlers = Handlers::default();
        add_probe_routes(&mut handlers);
        api.register(&mut handlers);
        let service = RelayService::new(
            handlers,
            HeaderMap::new(),
            None,
            KeyCache::new(KEY_CACHE_CAPACITY),
            access.clone(),
            Arc::new(Metrics::default()),
        );
        access.attach(service.clients().clone());

        tasks.spawn(accept_loop(
            listener,
            service.clone(),
            TlsConfig::new(server_config.clone()),
        ));
        tasks.spawn(check_keys(access.clone()));

        let quic = match options.quic_bind {
            Some(bind) => start_quic(bind, (*server_config).clone()).await,
            None => None,
        };
        let quic_addr = quic.as_ref().and_then(|server| server.quic_addr());
        Ok(Self {
            https_addr,
            quic_addr,
            service,
            access,
            store,
            resolver,
            quic,
            tasks,
        })
    }

    pub fn https_addr(&self) -> SocketAddr {
        self.https_addr
    }

    pub fn quic_addr(&self) -> Option<SocketAddr> {
        self.quic_addr
    }

    pub fn access(&self) -> &Arc<RelayAccess> {
        &self.access
    }

    pub fn store(&self) -> &Arc<KeyStore> {
        &self.store
    }

    pub fn has_certificate(&self) -> bool {
        self.resolver.current().is_some()
    }

    /// Stops accepting, closes every relay connection, and stops QUIC.
    pub async fn shutdown(mut self) {
        self.tasks.shutdown().await;
        self.service.shutdown().await;
        if let Some(quic) = self.quic.take() {
            let _ = quic.shutdown().await;
        }
    }
}

/// Binds every address of `bind`'s family, falling back from IPv6 to IPv4 on hosts
/// without IPv6.
async fn bind_tcp(bind: SocketAddr) -> std::io::Result<TcpListener> {
    match TcpListener::bind(bind).await {
        Err(error) if bind.ip() == Ipv6Addr::UNSPECIFIED => {
            log::event(
                "serve.bind",
                json!({ "outcome": "ipv4_only", "errorClass": log::io_class(&error) }),
            );
            TcpListener::bind((Ipv4Addr::UNSPECIFIED, bind.port())).await
        }
        result => result,
    }
}

async fn accept_loop(listener: TcpListener, service: RelayService, tls_config: TlsConfig) {
    // Connection tasks live in this set, so stopping the loop stops them too.
    let mut connections = JoinSet::new();
    loop {
        while connections.try_join_next().is_some() {}
        match listener.accept().await {
            Ok((stream, _peer)) => {
                let service = service.clone();
                let tls_config = tls_config.clone();
                connections.spawn(async move {
                    service
                        .handle_connection(stream, Some(tls_config), ESTABLISH_TIMEOUT)
                        .await;
                });
            }
            Err(error) => {
                log::event(
                    "serve.accept",
                    json!({ "outcome": "error", "errorClass": log::io_class(&error) }),
                );
                tokio::time::sleep(ACCEPT_RETRY_PAUSE).await;
            }
        }
    }
}

/// Cuts connections whose key an operator's command revoked. A check reads only the key
/// file's metadata unless the file changed, so it runs quietly every few seconds.
async fn check_keys(access: Arc<RelayAccess>) {
    loop {
        tokio::time::sleep(KEY_CHECK_INTERVAL).await;
        access.cut_revoked();
    }
}

/// Starts QUIC address discovery. It is optional for iroh clients, so a failure is logged
/// and the relay goes on without it.
async fn start_quic(
    bind: SocketAddr,
    server_config: rustls::ServerConfig,
) -> Option<iroh_relay::server::Server> {
    let started = Instant::now();
    let mut binds = vec![bind];
    if bind.ip() == Ipv6Addr::UNSPECIFIED {
        binds.push((Ipv4Addr::UNSPECIFIED, bind.port()).into());
    }
    for bind in binds {
        let mut quic = QuicConfig::new(bind);
        quic.server_config = Some(server_config.clone());
        let mut config = iroh_relay::server::ServerConfig::default();
        config.quic = Some(quic);
        match iroh_relay::server::Server::spawn(config).await {
            Ok(server) => {
                log::event(
                    "serve.quic",
                    json!({
                        "outcome": "ok",
                        "port": server.quic_addr().map(|addr| addr.port()),
                        "durationMs": log::millis_since(started),
                    }),
                );
                return Some(server);
            }
            Err(_) => {
                // iroh's error text may hold the address; the class is enough.
                log::event(
                    "serve.quic",
                    json!({ "outcome": "error", "errorClass": "spawn_failed", "ipv6": bind.is_ipv6() }),
                );
            }
        }
    }
    None
}

/// The probes iroh clients use, and a page for a person who opens the address in a browser.
// TODO: iroh's own relay also answers `/generate_204` on plain HTTP port 80, which clients
// use to detect captive portals after HTTPS probes fail. It is left out because port 80 is
// often taken and the relay works without it; add it if clients on hotel or airport
// networks report trouble.
fn add_probe_routes(handlers: &mut Handlers) {
    // iroh clients measure latency to the relay with this path.
    handlers.insert(
        (Method::GET, RELAY_PROBE_PATH),
        Box::new(|_request, response| {
            let body: BytesBody = Box::new(Full::new(Bytes::new()));
            Ok(response
                .status(StatusCode::OK)
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(body)?)
        }),
    );
    handlers.insert(
        (Method::GET, "/healthz"),
        Box::new(|_request, response| text_response(response, "ok\n")),
    );
    handlers.insert(
        (Method::GET, "/"),
        Box::new(|_request, response| text_response(response, INDEX_TEXT)),
    );
}

fn text_response(
    response: http::response::Builder,
    text: &'static str,
) -> iroh_relay::server::http_server::HyperResult<http::Response<BytesBody>> {
    let body: BytesBody = Box::new(Full::new(Bytes::from_static(text.as_bytes())));
    Ok(response
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body)?)
}

/// The `serve` command: runs the relay `setup` configured until the service is stopped.
pub async fn serve(state_dir: PathBuf) -> Result<(), String> {
    log::install_iroh_diagnostics();
    // The ACME client builds its TLS config from the process default provider. Today ring
    // is the only one compiled in, so rustls would pick it anyway; this keeps it so.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = match RelayConfig::load(&state_dir) {
        Ok(Some(config)) => config,
        Ok(None) => {
            log::event(
                "serve.start",
                json!({ "outcome": "error", "errorClass": "not_set_up" }),
            );
            return Err("This relay is not set up yet. Run: sudo openaide-relay setup".into());
        }
        Err(error) => {
            log::event(
                "serve.start",
                json!({ "outcome": "error", "errorClass": log::io_class(&error) }),
            );
            return Err(format!("Cannot read the relay's settings: {error}"));
        }
    };
    let options = ServeOptions::from_config(state_dir, &config);
    let relay = RunningRelay::start(options).await.map_err(|error| {
        log::event(
            "serve.start",
            json!({ "outcome": "error", "errorClass": log::io_class(&error) }),
        );
        format!("Cannot start the relay: {error}")
    })?;
    log::event(
        "serve.start",
        json!({
            "outcome": "ok",
            "port": relay.https_addr().port(),
            "quicPort": relay.quic_addr().map(|addr| addr.port()),
            "certSource": match config.cert { CertSource::Acme => "acme", CertSource::Files { .. } => "files" },
            "hostKind": match config.host { Host::Name(_) => "name", Host::Ip(_) => "ip" },
        }),
    );
    wait_for_stop_signal().await;
    log::event("serve.stop", json!({ "outcome": "start" }));
    relay.shutdown().await;
    log::event("serve.stop", json!({ "outcome": "ok" }));
    Ok(())
}

async fn wait_for_stop_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    match signal(SignalKind::terminate()) {
        Ok(mut terminate) => {
            tokio::select! {
                _ = terminate.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
        }
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
