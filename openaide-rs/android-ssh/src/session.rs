//! One SSH session to an SSH Computer, opened again on demand.
//!
//! Android stops a hidden app's network without telling it, so a session is
//! not kept healthy in the background: whoever needs it next asks
//! [`Connector::session`], which opens a new one when the last has closed.
//! Every session checks the host key against the one the user accepted.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::client::{self, AuthResult, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::MethodKind;
use serde_json::json;

use crate::Log;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Where the computer is, as the user entered it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
}

/// How a session signs in.
#[derive(Clone)]
pub(crate) enum Credential {
    Key(Arc<PrivateKey>),
    /// Used once to add the phone's key, then forgotten.
    Password(String),
}

/// Why a session could not be opened, as a stable class.
pub(crate) type Class = &'static str;

pub(crate) struct Connector {
    target: Target,
    host_key: PublicKey,
    credential: Credential,
    log: Log,
    session: tokio::sync::Mutex<Option<Arc<Handle<PinnedHost>>>>,
    opened: Mutex<u32>,
}

impl Connector {
    pub(crate) fn new(
        target: Target,
        host_key: PublicKey,
        credential: Credential,
        log: Log,
    ) -> Self {
        Self {
            target,
            host_key,
            credential,
            log,
            session: tokio::sync::Mutex::new(None),
            opened: Mutex::new(0),
        }
    }

    /// The open session, or a new one when there is none or it closed.
    pub(crate) async fn session(&self) -> Result<Arc<Handle<PinnedHost>>, Class> {
        let mut slot = self.session.lock().await;
        if let Some(handle) = slot.as_ref().filter(|handle| !handle.is_closed()) {
            return Ok(handle.clone());
        }
        let attempt = {
            let mut opened = self.opened.lock().map_err(|_| "ssh_failed")?;
            *opened += 1;
            *opened
        };
        let started = Instant::now();
        (self.log)("ssh_session_open_started", json!({ "attempt": attempt }));
        let result = open(&self.target, &self.host_key, &self.credential).await;
        (self.log)(
            "ssh_session_open_completed",
            json!({
                "attempt": attempt,
                "outcome": if result.is_ok() { "success" } else { "failure" },
                "errorClass": result.as_ref().err(),
                "durationMs": started.elapsed().as_millis() as u64,
            }),
        );
        let handle = Arc::new(result?);
        *slot = Some(handle.clone());
        Ok(handle)
    }

    /// Ends the session; the next use opens a new one.
    pub(crate) async fn close(&self) {
        if let Some(handle) = self.session.lock().await.take() {
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        }
    }
}

/// Reads the computer's host key without signing in, for the user to accept.
pub(crate) async fn read_host_key(target: &Target) -> Result<PublicKey, Class> {
    let seen = Arc::new(Mutex::new(None));
    let handler = PinnedHost {
        expected: None,
        seen: seen.clone(),
    };
    let handle = connect(target, handler).await?;
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;
    let key = seen.lock().map_err(|_| "ssh_failed")?.take();
    key.ok_or("ssh_failed")
}

async fn open(
    target: &Target,
    host_key: &PublicKey,
    credential: &Credential,
) -> Result<Handle<PinnedHost>, Class> {
    let handler = PinnedHost {
        expected: Some(host_key.clone()),
        seen: Arc::default(),
    };
    let mut handle = connect(target, handler).await?;
    let signed_in = tokio::time::timeout(CONNECT_TIMEOUT, sign_in(&mut handle, target, credential))
        .await
        .map_err(|_| "unreachable")??;
    if signed_in {
        Ok(handle)
    } else {
        Err("auth_failed")
    }
}

async fn connect(target: &Target, handler: PinnedHost) -> Result<Handle<PinnedHost>, Class> {
    let mut addresses = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::lookup_host((target.host.as_str(), target.port)),
    )
    .await
    .map_err(|_| "unreachable")?
    .map_err(|_| "host_unknown")?;
    let address = addresses.next().ok_or("host_unknown")?;
    let config = Arc::new(client::Config {
        // Notices a dead connection while a command or forward is in use.
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });
    tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, address, handler))
        .await
        .map_err(|_| "unreachable")?
        .map_err(|error| classify(&error))
}

async fn sign_in(
    handle: &mut Handle<PinnedHost>,
    target: &Target,
    credential: &Credential,
) -> Result<bool, Class> {
    match credential {
        Credential::Key(key) => {
            let key = PrivateKeyWithHashAlg::new(key.clone(), None);
            let result = handle
                .authenticate_publickey(&target.user, key)
                .await
                .map_err(|error| classify(&error))?;
            Ok(matches!(result, AuthResult::Success))
        }
        Credential::Password(password) => {
            let result = handle
                .authenticate_password(&target.user, password)
                .await
                .map_err(|error| classify(&error))?;
            match result {
                AuthResult::Success => Ok(true),
                // Many servers take passwords only through PAM's prompts.
                AuthResult::Failure {
                    remaining_methods, ..
                } if remaining_methods.contains(&MethodKind::KeyboardInteractive) => {
                    keyboard_interactive(handle, target, password).await
                }
                AuthResult::Failure { .. } => Ok(false),
            }
        }
    }
}

/// Answers every hidden prompt with the password; a visible prompt, such as a
/// one-time code, is not something this flow can answer.
async fn keyboard_interactive(
    handle: &mut Handle<PinnedHost>,
    target: &Target,
    password: &str,
) -> Result<bool, Class> {
    let mut response = handle
        .authenticate_keyboard_interactive_start(&target.user, None)
        .await
        .map_err(|error| classify(&error))?;
    for _ in 0..4 {
        match response {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                if prompts.iter().any(|prompt| prompt.echo) {
                    return Err("auth_prompt_unsupported");
                }
                let answers = prompts.iter().map(|_| password.to_string()).collect();
                response = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|error| classify(&error))?;
            }
        }
    }
    Ok(false)
}

/// Accepts only the expected host key, and remembers the key it was shown.
pub(crate) struct PinnedHost {
    expected: Option<PublicKey>,
    seen: Arc<Mutex<Option<PublicKey>>>,
}

impl client::Handler for PinnedHost {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Ok(false);
        };
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(key.clone());
        }
        Ok(match &self.expected {
            Some(expected) => expected.key_data() == key.key_data(),
            None => true,
        })
    }
}

/// Maps a russh failure to a class; its text is never kept.
pub(crate) fn classify(error: &russh::Error) -> Class {
    match error {
        russh::Error::IO(error) => match error.kind() {
            std::io::ErrorKind::ConnectionRefused => "connection_refused",
            std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::HostUnreachable
            | std::io::ErrorKind::NetworkUnreachable => "unreachable",
            _ => "connection_lost",
        },
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout
        | russh::Error::Elapsed(_) => "unreachable",
        russh::Error::Disconnect | russh::Error::HUP | russh::Error::SendError => "connection_lost",
        russh::Error::ChannelOpenFailure(_) => "forward_failed",
        // The handler refused a key other than the accepted one.
        russh::Error::UnknownKey => "host_key_changed",
        russh::Error::NoCommonAlgo { .. } | russh::Error::UnknownAlgo => "unsupported_algorithm",
        _ => "ssh_failed",
    }
}
