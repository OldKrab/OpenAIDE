//! Automatic certificates from Let's Encrypt over TLS-ALPN-01.
//!
//! The relay proves it holds its address by answering a validation handshake on port 443
//! (see [`crate::tls::CertResolver`]), so it needs no port 80 and no web server. A DNS name
//! gets Let's Encrypt's default profile. A bare IP address can only get a short-lived
//! certificate (about 6 days, the `shortlived` profile), which is why renewal is automatic
//! and starts once a third of the lifetime is left.
//!
//! The account and the current certificate live under `<state dir>/acme/`. A failed
//! renewal leaves the old certificate in service and retries with growing pauses.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, RetryPolicy,
};
use rustls::sign::CertifiedKey;
use rustls_pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::Host;
use crate::files;
use crate::log;
use crate::tls::{self, CertResolver};

pub const LETS_ENCRYPT_PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
/// Let's Encrypt's profile for 6-day certificates, the only one that covers an IP address.
pub const SHORT_LIVED_PROFILE: &str = "shortlived";
const ACME_DIR: &str = "acme";
const ACCOUNT_FILE: &str = "account.json";
const CERTIFICATE_FILE: &str = "certificate.json";

/// The first pause after a failed attempt; it doubles up to [`MAX_RETRY_PAUSE`].
pub const FIRST_RETRY_PAUSE: Duration = Duration::from_secs(60);
/// Let's Encrypt allows only a few failed validations per hour, so retries slow to this.
pub const MAX_RETRY_PAUSE: Duration = Duration::from_secs(60 * 60);
/// A long wait for renewal is taken in steps of at most this, so a changed clock is noticed.
const MAX_SLEEP: Duration = Duration::from_secs(60 * 60);
/// How long an order may take to be validated, and then to be issued.
const ORDER_POLL: RetryPolicy = RetryPolicy::new().timeout(Duration::from_secs(120));

/// What the relay asks a certificate for.
pub fn identifier(host: &Host) -> Identifier {
    match host {
        Host::Name(name) => Identifier::Dns(name.clone()),
        Host::Ip(ip) => Identifier::Ip(*ip),
    }
}

/// The ACME profile for `host`; `None` is the directory's default.
pub fn profile(host: &Host) -> Option<&'static str> {
    match host {
        Host::Name(_) => None,
        Host::Ip(_) => Some(SHORT_LIVED_PROFILE),
    }
}

/// When to renew a certificate valid from `not_before` to `not_after`: once a third of
/// its lifetime is left.
// TODO: also ask Let's Encrypt's renewal information (ACME ARI, `Account::renewal_info`)
// and renew early when it says so, as it does after a mass revocation. The fixed third
// covers normal expiry only.
pub fn renew_at(not_before: SystemTime, not_after: SystemTime) -> SystemTime {
    let lifetime = not_after
        .duration_since(not_before)
        .unwrap_or(Duration::ZERO);
    not_after - lifetime / 3
}

/// The pause after `failures` failed attempts in a row (at least one).
pub fn retry_pause(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    FIRST_RETRY_PAUSE
        .saturating_mul(1 << doublings)
        .min(MAX_RETRY_PAUSE)
}

/// The TLS-ALPN-01 validation certificate (RFC 8737) for `host`: self-signed, naming the
/// host, with the key authorization's SHA-256 in the `acmeIdentifier` extension.
pub fn challenge_certificate(
    host: &Host,
    key_authorization_digest: &[u8],
) -> Result<Arc<CertifiedKey>, rcgen::Error> {
    let mut params = rcgen::CertificateParams::new(vec![host.to_string()])?;
    params.custom_extensions = vec![rcgen::CustomExtension::new_acme_identifier(
        key_authorization_digest,
    )];
    let key = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    let private = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    // Not `CertifiedKey::from_der`: its key check parses the certificate with webpki,
    // which rejects the critical `acmeIdentifier` extension this certificate must carry.
    let signing_key = tls::provider()
        .key_provider
        .load_private_key(private)
        .map_err(|_| rcgen::Error::CouldNotParseKeyPair)?;
    Ok(Arc::new(CertifiedKey::new(
        vec![cert.der().clone()],
        signing_key,
    )))
}

#[derive(Debug, Clone)]
pub struct AcmeSettings {
    pub host: Host,
    pub directory: String,
    pub state_dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    directory: String,
    credentials: AccountCredentials,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCertificate {
    host: Host,
    chain_pem: String,
    key_pem: String,
}

/// A failed step, with a class that is safe to log.
#[derive(Debug)]
struct AcmeFailure {
    step: &'static str,
    class: String,
}

impl AcmeFailure {
    fn new(step: &'static str, class: impl Into<String>) -> Self {
        Self {
            step,
            class: class.into(),
        }
    }

    fn api(step: &'static str) -> impl FnOnce(instant_acme::Error) -> Self {
        move |error| Self::new(step, acme_error_class(&error))
    }

    fn io(step: &'static str) -> impl FnOnce(io::Error) -> Self {
        move |error| Self::new(step, log::io_class(&error))
    }
}

/// An ACME problem type is a fixed URN, so its last part is a safe class.
fn acme_error_class(error: &instant_acme::Error) -> String {
    match error {
        instant_acme::Error::Api(problem) => problem
            .r#type
            .as_deref()
            .and_then(|kind| kind.rsplit(':').next())
            .map(|kind| {
                let kind: String = kind
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .take(40)
                    .collect();
                format!("acme:{kind}")
            })
            .unwrap_or_else(|| "acme:unknown".to_string()),
        instant_acme::Error::Http(_) | instant_acme::Error::Hyper(_) => "network".to_string(),
        instant_acme::Error::Timeout(_) => "timed_out".to_string(),
        instant_acme::Error::Unsupported(_) => "unsupported".to_string(),
        _ => "acme_client".to_string(),
    }
}

/// Keeps the relay's certificate current, forever. Installs a stored certificate at once,
/// then issues or renews as needed.
pub async fn run(settings: AcmeSettings, resolver: Arc<CertResolver>) {
    let mut renew_after = match load_certificate(&settings) {
        Some((key, renew)) => {
            resolver.set_current(key);
            renew
        }
        None => SystemTime::now(),
    };
    loop {
        wait_until(renew_after).await;
        let mut failures = 0;
        renew_after = loop {
            match issue(&settings, &resolver).await {
                Ok(renew) => break renew,
                Err(failure) => {
                    failures += 1;
                    let pause = retry_pause(failures);
                    log::event(
                        "acme.issue",
                        json!({
                            "outcome": "error",
                            "step": failure.step,
                            "errorClass": failure.class,
                            "attempt": failures,
                            "retryInSeconds": pause.as_secs(),
                            "servingPrevious": resolver.current().is_some(),
                        }),
                    );
                    tokio::time::sleep(pause).await;
                }
            }
        };
    }
}

async fn wait_until(when: SystemTime) {
    while let Ok(left) = when.duration_since(SystemTime::now()) {
        if left.is_zero() {
            break;
        }
        tokio::time::sleep(left.min(MAX_SLEEP)).await;
    }
}

/// The stored certificate and its renewal time, if it is for this host and readable.
fn load_certificate(settings: &AcmeSettings) -> Option<(Arc<CertifiedKey>, SystemTime)> {
    let path = acme_dir(&settings.state_dir).join(CERTIFICATE_FILE);
    let stored: StoredCertificate = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    if stored.host != settings.host {
        log::event("acme.stored", json!({ "outcome": "other_host" }));
        return None;
    }
    let key =
        tls::certified_key_from_pem(stored.chain_pem.as_bytes(), stored.key_pem.as_bytes()).ok()?;
    let (not_before, not_after) = tls::validity(key.end_entity_cert().ok()?)?;
    log::event(
        "acme.stored",
        json!({ "outcome": "ok", "expiresInHours": tls::hours_until(not_after) }),
    );
    Some((key, renew_at(not_before, not_after)))
}

/// One complete issuance: order, validate, finalize, store, install. Returns when to renew.
async fn issue(
    settings: &AcmeSettings,
    resolver: &CertResolver,
) -> Result<SystemTime, AcmeFailure> {
    let started = Instant::now();
    log::event(
        "acme.issue",
        json!({ "outcome": "start", "profile": profile(&settings.host) }),
    );
    let account = account(settings).await?;
    let identifiers = [identifier(&settings.host)];
    let mut new_order = NewOrder::new(&identifiers);
    if let Some(profile) = profile(&settings.host) {
        new_order = new_order.profile(profile);
    }
    let mut order = account
        .new_order(&new_order)
        .await
        .map_err(AcmeFailure::api("new_order"))?;

    let validated = validate(&settings.host, &mut order, resolver).await;
    // The validation certificate is only for this order.
    resolver.set_challenge(None);
    validated?;
    log::event(
        "acme.validated",
        json!({ "durationMs": log::millis_since(started) }),
    );

    let key_pem = order
        .finalize()
        .await
        .map_err(AcmeFailure::api("finalize"))?;
    let chain_pem = order
        .poll_certificate(&ORDER_POLL)
        .await
        .map_err(AcmeFailure::api("certificate"))?;
    let key = tls::certified_key_from_pem(chain_pem.as_bytes(), key_pem.as_bytes())
        .map_err(|error| AcmeFailure::new("certificate", error.class()))?;
    let (not_before, not_after) = key
        .end_entity_cert()
        .ok()
        .and_then(tls::validity)
        .ok_or_else(|| AcmeFailure::new("certificate", "unreadable_validity"))?;

    let stored = StoredCertificate {
        host: settings.host.clone(),
        chain_pem,
        key_pem,
    };
    let text = serde_json::to_vec(&stored).map_err(|_| AcmeFailure::new("store", "encode"))?;
    write_acme_file(&settings.state_dir, CERTIFICATE_FILE, &text)
        .map_err(AcmeFailure::io("store"))?;
    resolver.set_current(key);
    log::event(
        "acme.issue",
        json!({
            "outcome": "ok",
            "durationMs": log::millis_since(started),
            "expiresInHours": tls::hours_until(not_after),
        }),
    );
    Ok(renew_at(not_before, not_after))
}

/// Answers every pending authorization with TLS-ALPN-01 and waits for the order to be ready.
async fn validate(
    host: &Host,
    order: &mut instant_acme::Order,
    resolver: &CertResolver,
) -> Result<(), AcmeFailure> {
    let mut authorizations = order.authorizations();
    while let Some(authorization) = authorizations.next().await {
        let mut authorization = authorization.map_err(AcmeFailure::api("authorization"))?;
        match authorization.status {
            AuthorizationStatus::Pending => {}
            AuthorizationStatus::Valid => continue,
            _ => return Err(AcmeFailure::new("authorization", "not_pending")),
        }
        let mut challenge = authorization
            .challenge(ChallengeType::TlsAlpn01)
            .ok_or_else(|| AcmeFailure::new("authorization", "no_tls_alpn_challenge"))?;
        let digest = challenge.key_authorization().digest();
        let certificate = challenge_certificate(host, digest.as_ref())
            .map_err(|_| AcmeFailure::new("challenge", "certificate"))?;
        resolver.set_challenge(Some(certificate));
        challenge
            .set_ready()
            .await
            .map_err(AcmeFailure::api("challenge"))?;
    }
    let status = order
        .poll_ready(&ORDER_POLL)
        .await
        .map_err(AcmeFailure::api("validation"))?;
    if status != OrderStatus::Ready {
        return Err(AcmeFailure::new(
            "validation",
            format!("order_{}", order_status_name(status)),
        ));
    }
    Ok(())
}

fn order_status_name(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::Pending => "pending",
        OrderStatus::Ready => "ready",
        OrderStatus::Processing => "processing",
        OrderStatus::Valid => "valid",
        OrderStatus::Invalid => "invalid",
    }
}

/// The stored account for this directory, or a new one.
async fn account(settings: &AcmeSettings) -> Result<Account, AcmeFailure> {
    let path = acme_dir(&settings.state_dir).join(ACCOUNT_FILE);
    let stored = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredAccount>(&bytes).ok())
        .filter(|stored| stored.directory == settings.directory);
    let builder = Account::builder().map_err(AcmeFailure::api("account"))?;
    if let Some(stored) = stored {
        return builder
            .from_credentials(stored.credentials)
            .await
            .map_err(AcmeFailure::api("account"));
    }
    let (account, credentials) = builder
        .create(
            &NewAccount {
                contact: &[],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            settings.directory.clone(),
            None,
        )
        .await
        .map_err(AcmeFailure::api("account"))?;
    let stored = StoredAccount {
        directory: settings.directory.clone(),
        credentials,
    };
    let text = serde_json::to_vec(&stored).map_err(|_| AcmeFailure::new("account", "encode"))?;
    write_acme_file(&settings.state_dir, ACCOUNT_FILE, &text)
        .map_err(AcmeFailure::io("account"))?;
    log::event("acme.account", json!({ "outcome": "created" }));
    Ok(account)
}

fn acme_dir(state_dir: &Path) -> PathBuf {
    state_dir.join(ACME_DIR)
}

fn write_acme_file(state_dir: &Path, name: &str, contents: &[u8]) -> io::Result<()> {
    let dir = acme_dir(state_dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    files::write_private(&dir.join(name), contents)
}

#[cfg(test)]
#[path = "acme_tests.rs"]
mod tests;
