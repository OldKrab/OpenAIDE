//! TLS for the relay's HTTPS port and its QUIC port.
//!
//! One [`CertResolver`] owns the certificate both ports present. Whoever obtains a
//! certificate (the ACME task, or the poller of a certificate directory) installs it there,
//! and the next handshake uses it; open connections keep the one they started with. While
//! an ACME order is open, the resolver also answers TLS-ALPN-01 validation handshakes.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use rustls::crypto::CryptoProvider;
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use serde_json::json;

use crate::log;

/// The ALPN protocol of a TLS-ALPN-01 validation handshake (RFC 8737).
pub const ACME_TLS_ALPN: &[u8] = b"acme-tls/1";
/// What iroh's relay clients and probes speak over TLS.
pub const HTTP_1_1_ALPN: &[u8] = b"http/1.1";

/// The ring crypto provider, which the rest of the workspace uses too.
pub fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The certificate the relay presents, and the validation certificate while an ACME
/// order is open.
#[derive(Debug, Default)]
pub struct CertResolver {
    current: RwLock<Option<Arc<CertifiedKey>>>,
    challenge: RwLock<Option<Arc<CertifiedKey>>>,
}

impl CertResolver {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn current(&self) -> Option<Arc<CertifiedKey>> {
        self.current
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    pub fn set_current(&self, key: Arc<CertifiedKey>) {
        *self
            .current
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(key);
    }

    pub fn set_challenge(&self, key: Option<Arc<CertifiedKey>>) {
        *self
            .challenge
            .write()
            .unwrap_or_else(|poison| poison.into_inner()) = key;
    }

    /// The certificate for a handshake that does or does not offer `acme-tls/1`.
    pub fn choose(&self, offers_acme: bool) -> Option<Arc<CertifiedKey>> {
        let slot = if offers_acme {
            // RFC 8737 §3: such a handshake gets the validation certificate or nothing.
            &self.challenge
        } else {
            &self.current
        };
        slot.read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.choose(offers_acme_alpn(client_hello.alpn()))
    }
}

/// Whether a ClientHello's ALPN list names `acme-tls/1`.
pub fn offers_acme_alpn<'a>(alpn: Option<impl Iterator<Item = &'a [u8]>>) -> bool {
    alpn.is_some_and(|mut protocols| protocols.any(|protocol| protocol == ACME_TLS_ALPN))
}

/// The server config both ports share. The QUIC server copies it and sets its own ALPN.
pub fn server_config(resolver: Arc<CertResolver>) -> io::Result<rustls::ServerConfig> {
    let mut config = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    config.alpn_protocols = vec![HTTP_1_1_ALPN.to_vec(), ACME_TLS_ALPN.to_vec()];
    Ok(config)
}

/// Why a certificate could not be used. Safe to log by [`CertError::class`].
#[derive(Debug)]
pub enum CertError {
    Io(io::Error),
    /// No certificate in the PEM text.
    NoCertificate,
    /// No private key in the PEM text, or one of an unsupported type.
    BadKey,
    /// The key does not belong to the certificate.
    KeyMismatch,
}

impl CertError {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Io(error) => log::io_class(error),
            Self::NoCertificate => "no_certificate",
            Self::BadKey => "bad_key",
            Self::KeyMismatch => "key_mismatch",
        }
    }
}

impl std::fmt::Display for CertError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "cannot read the certificate files: {error}"),
            Self::NoCertificate => formatter.write_str("no certificate in the file"),
            Self::BadKey => formatter.write_str("no usable private key in the file"),
            Self::KeyMismatch => formatter.write_str("the key does not match the certificate"),
        }
    }
}

/// Builds a certificate from a PEM chain (leaf first) and a PEM private key.
pub fn certified_key_from_pem(
    chain_pem: &[u8],
    key_pem: &[u8],
) -> Result<Arc<CertifiedKey>, CertError> {
    let chain = CertificateDer::pem_slice_iter(chain_pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CertError::NoCertificate)?;
    if chain.is_empty() {
        return Err(CertError::NoCertificate);
    }
    let key = PrivateKeyDer::from_pem_slice(key_pem).map_err(|_| CertError::BadKey)?;
    certified_key_from_der(chain, key)
}

pub fn certified_key_from_der(
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<CertifiedKey>, CertError> {
    let certified =
        CertifiedKey::from_der(chain, key, &provider()).map_err(|error| match error {
            rustls::Error::InconsistentKeys(_) => CertError::KeyMismatch,
            _ => CertError::BadKey,
        })?;
    Ok(Arc::new(certified))
}

/// When a certificate's validity starts and ends.
pub fn validity(cert: &CertificateDer<'_>) -> Option<(SystemTime, SystemTime)> {
    let (_, parsed) = x509_parser::parse_x509_certificate(cert.as_ref()).ok()?;
    let at = |seconds: i64| {
        let seconds = u64::try_from(seconds).ok()?;
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
    };
    Some((
        at(parsed.validity().not_before.timestamp())?,
        at(parsed.validity().not_after.timestamp())?,
    ))
}

/// The certificate and key files in a directory: certbot's names first, then iroh's.
pub fn cert_files(dir: &Path) -> (PathBuf, PathBuf) {
    let certbot = (dir.join("fullchain.pem"), dir.join("privkey.pem"));
    if certbot.0.exists() || !dir.join("default.crt").exists() {
        return certbot;
    }
    (dir.join("default.crt"), dir.join("default.key"))
}

pub fn load_cert_dir(dir: &Path) -> Result<Arc<CertifiedKey>, CertError> {
    let (chain_path, key_path) = cert_files(dir);
    let chain = std::fs::read(chain_path).map_err(CertError::Io)?;
    let key = std::fs::read(key_path).map_err(CertError::Io)?;
    certified_key_from_pem(&chain, &key)
}

/// How often a certificate directory is checked for renewed files.
pub const CERT_DIR_POLL: Duration = Duration::from_secs(5 * 60);
/// How often it is checked while the relay has no certificate yet.
const CERT_DIR_POLL_WITHOUT_CERT: Duration = Duration::from_secs(10);

/// The SHA-256 of both files' bytes, or `None` while either cannot be read.
type DirStamp = Option<[u8; 32]>;

/// Keeps the certificate from a directory that something else renews: loads it, then
/// rereads it whenever the files change. A load that fails leaves the previous certificate
/// in place.
#[derive(Debug)]
pub struct CertDirWatcher {
    dir: PathBuf,
    resolver: Arc<CertResolver>,
    /// The files' stamp at the last successful load; `None` until one succeeds, so a
    /// missing file is tried again on every check.
    loaded: Option<DirStamp>,
    /// The last failure reported, so a repeat of it stays quiet.
    last_error: Option<&'static str>,
}

impl CertDirWatcher {
    pub fn new(dir: PathBuf, resolver: Arc<CertResolver>) -> Self {
        Self {
            dir,
            resolver,
            loaded: None,
            last_error: None,
        }
    }

    /// Loads the files if they changed since the last successful load.
    pub fn check(&mut self) {
        let stamp = dir_stamp(&self.dir);
        if self.loaded.as_ref() == Some(&stamp) {
            return;
        }
        let started = Instant::now();
        match load_cert_dir(&self.dir) {
            Ok(key) => {
                let not_after = key.end_entity_cert().ok().and_then(validity);
                self.resolver.set_current(key);
                self.loaded = Some(stamp);
                self.last_error = None;
                log::event(
                    "cert.load",
                    json!({
                        "outcome": "ok",
                        "source": "files",
                        "durationMs": log::millis_since(started),
                        "expiresInHours": not_after.map(|(_, not_after)| hours_until(not_after)),
                    }),
                );
            }
            // Retried on the next check even if the files stay the same, because the
            // cause may be a permission the operator fixes.
            Err(error) if self.last_error != Some(error.class()) => {
                self.last_error = Some(error.class());
                log::event(
                    "cert.load",
                    json!({
                        "outcome": "error",
                        "source": "files",
                        "errorClass": error.class(),
                        "servingPrevious": self.resolver.current().is_some(),
                    }),
                );
            }
            Err(_) => {}
        }
    }

    pub async fn run(mut self) {
        loop {
            let wait = if self.resolver.current().is_some() {
                CERT_DIR_POLL
            } else {
                CERT_DIR_POLL_WITHOUT_CERT
            };
            tokio::time::sleep(wait).await;
            self.check();
        }
    }
}

/// Changes whenever either file's content does. Hashing the bytes catches a rewrite in
/// place that a coarse modification time could miss. Reading follows symlinks, as
/// certbot's `live/` directory holds links into `archive/`.
fn dir_stamp(dir: &Path) -> DirStamp {
    use sha2::{Digest, Sha256};
    let (chain, key) = cert_files(dir);
    let mut hasher = Sha256::new();
    for path in [chain, key] {
        let bytes = std::fs::read(path).ok()?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Some(hasher.finalize().into())
}

pub fn hours_until(time: SystemTime) -> i64 {
    match time.duration_since(SystemTime::now()) {
        Ok(left) => i64::try_from(left.as_secs() / 3600).unwrap_or(i64::MAX),
        Err(past) => -i64::try_from(past.duration().as_secs() / 3600).unwrap_or(i64::MAX),
    }
}

#[cfg(test)]
#[path = "tls_tests.rs"]
pub(crate) mod tests;
