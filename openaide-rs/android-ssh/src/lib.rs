//! SSH Computers for the Mobile App (ADR-0069).
//!
//! The Kotlin shell owns the phone's key (encrypted with Android Keystore), the
//! accepted host key, the release download, and every question to the user.
//! This library owns the SSH protocol and runs the shared [`SshLauncher`] over
//! it, then keeps a loopback forward to the App Server while the shell holds
//! the [`SshComputer`]. Nothing here is persisted, and logs carry only stable
//! names, classes, counts, and durations.

mod session;
mod transport;

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use openaide_remote_client::{
    archive, LaunchError, Launched, Logger, ServerRelease, SshLauncher, StartMode, StopOutcome,
};
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};
use russh::keys::{HashAlg, PrivateKey, PublicKey};
use tokio::runtime::Runtime;

pub use session::Target;
use session::{Connector, Credential};
use transport::{EmbeddedSsh, Forward};

uniffi::setup_scaffolding!();

pub(crate) type Log = Logger;

/// Why an SSH operation failed: the launch stage and a stable class, such as
/// `auth_failed`, `host_key_changed`, or `unreachable`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SshError {
    #[error("{stage}: {class}")]
    Failed { stage: String, class: String },
}

impl SshError {
    fn new(stage: &str, class: &str) -> Self {
        Self::Failed {
            stage: stage.to_string(),
            class: class.to_string(),
        }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SshError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::new("fetch", "shell_callback_failed")
    }
}

/// What the shell provides while an SSH Computer is in use.
#[uniffi::export(with_foreign)]
pub trait SshEvents: Send + Sync {
    /// One metadata-only lifecycle event; `fields` is a JSON object.
    fn log(&self, event: String, fields: String);
    /// The launch reached `stage`: probe, fetch, upload, start, or forward.
    fn stage(&self, stage: String);
    /// Downloads this app's Linux App Server archive and returns its path.
    /// Called only when the computer and the cache both lack the release.
    fn release_archive(&self) -> Result<String, SshError>;
}

/// The App Server on the computer, reached through this phone's loopback.
#[derive(Debug, uniffi::Record)]
pub struct SshReady {
    /// `http://127.0.0.1:<port>/<path>`, as a Termux handoff line names it.
    pub endpoint_url: String,
    pub auth_token: String,
    pub app_version: String,
    pub update_available: bool,
    pub keeper_existed: bool,
    pub suggest_linger: bool,
}

#[derive(Debug, uniffi::Enum)]
pub enum SshLaunch {
    Ready {
        ready: SshReady,
    },
    /// An older App Server whose protocol this app cannot speak runs there;
    /// only a confirmed replacement can help.
    UpdateRequired {
        running_version: String,
    },
}

/// The SHA-256 fingerprint the user compares, such as `SHA256:…`.
#[uniffi::export]
pub fn host_key_fingerprint(host_key: String) -> Result<String, SshError> {
    let key = PublicKey::from_openssh(&host_key)
        .map_err(|_| SshError::new("trust", "host_key_invalid"))?;
    Ok(key.fingerprint(HashAlg::Sha256).to_string())
}

/// The phone's public key as an `authorized_keys` line.
#[uniffi::export]
pub fn public_key_line(seed: Vec<u8>, comment: String) -> Result<String, SshError> {
    let mut key = private_key(&seed)?;
    key.set_comment(comment);
    key.public_key()
        .to_openssh()
        .map_err(|_| SshError::new("key", "key_invalid"))
}

/// Connects without signing in and returns the computer's host key in
/// OpenSSH form, for the user to accept before anything else is sent.
#[uniffi::export]
pub fn read_host_key(target: Target) -> Result<String, SshError> {
    let key = runtime()
        .block_on(session::read_host_key(&target))
        .map_err(|class| SshError::new("trust", class))?;
    key.to_openssh()
        .map_err(|_| SshError::new("trust", "host_key_invalid"))
}

/// Signs in once with `password` and adds `public_key_line` to the user's
/// `authorized_keys`, unless it is there. The password is not kept.
#[uniffi::export]
pub fn authorize_key(
    target: Target,
    host_key: String,
    password: String,
    public_key_line: String,
    events: Arc<dyn SshEvents>,
) -> Result<(), SshError> {
    let stage = "authorize";
    if !is_key_line(&public_key_line) {
        return Err(SshError::new(stage, "key_invalid"));
    }
    let host_key = parse_host_key(&host_key)?;
    let connector = Arc::new(Connector::new(
        target,
        host_key,
        Credential::Password(password),
        logger(events.clone()),
    ));
    let mut transport = EmbeddedSsh {
        runtime: runtime().clone(),
        connector: connector.clone(),
        log: logger(events),
    };
    // One line, no `'` or `!`, so every common login shell passes it to `sh`.
    let command = format!(
        "sh -c 'umask 077; mkdir -p \"$HOME/.ssh\" && touch \"$HOME/.ssh/authorized_keys\" && {{ grep -qxF \"$1\" \"$HOME/.ssh/authorized_keys\" || printf \"%s\\n\" \"$1\" >> \"$HOME/.ssh/authorized_keys\"; }}' openaide '{public_key_line}'"
    );
    let result = openaide_remote_client::SshTransport::run(&mut transport, &command, None);
    runtime().block_on(connector.close());
    match result {
        Ok(output) if output.success => Ok(()),
        Ok(_) => Err(SshError::new(stage, "authorized_keys_unwritable")),
        Err(error) => Err(SshError::new(stage, error.class)),
    }
}

/// One SSH Computer while the shell uses it. Dropping or [`disconnect`](Self::disconnect)
/// ends its forward; the keeper on the computer stays by its own rules.
#[derive(uniffi::Object)]
pub struct SshComputer {
    connector: Arc<Connector>,
    events: Arc<dyn SshEvents>,
    release: ServerRelease,
    cache: PathBuf,
    forward: Mutex<Option<Forward>>,
}

#[uniffi::export]
impl SshComputer {
    /// `seed` is the phone's ed25519 key; `host_key` the accepted one, in
    /// OpenSSH form; `release_sha256` the Linux binary's, built into the app.
    #[uniffi::constructor]
    pub fn new(
        target: Target,
        host_key: String,
        seed: Vec<u8>,
        release_version: String,
        release_sha256: String,
        cache_directory: String,
        events: Arc<dyn SshEvents>,
    ) -> Result<Arc<Self>, SshError> {
        let release = ServerRelease::new(&release_version, &release_sha256)
            .ok_or_else(|| SshError::new("fetch", "unavailable_in_build"))?;
        let key = Arc::new(private_key(&seed)?);
        let connector = Arc::new(Connector::new(
            target,
            parse_host_key(&host_key)?,
            Credential::Key(key),
            logger(events.clone()),
        ));
        Ok(Arc::new(Self {
            connector,
            events,
            release,
            cache: PathBuf::from(cache_directory),
            forward: Mutex::new(None),
        }))
    }

    /// Starts or attaches to the App Server and forwards it. `replace` is the
    /// user's confirmed update of an older App Server.
    pub fn launch(&self, replace: bool, operation_id: String) -> Result<SshLaunch, SshError> {
        let events = self.events.clone();
        let launcher = SshLauncher::new(
            self.release.clone(),
            logger(self.events.clone()),
            Box::new(move |stage| events.stage(stage.as_str().to_string())),
        );
        let mut transport = self.transport();
        let mode = if replace {
            StartMode::Replace
        } else {
            StartMode::Keep
        };
        let mut fetch = || self.open_release();
        match launcher.launch(&mut transport, mode, &mut fetch, &operation_id) {
            Ok(Launched::Ready(ready)) => {
                if let Ok(mut forward) = self.forward.lock() {
                    *forward = Some(ready.forward);
                }
                Ok(SshLaunch::Ready {
                    ready: SshReady {
                        endpoint_url: ready.endpoint_url,
                        auth_token: ready.auth_token,
                        app_version: ready.app_version,
                        update_available: ready.update_available,
                        keeper_existed: ready.keeper_existed,
                        suggest_linger: ready.suggest_linger,
                    },
                })
            }
            Ok(Launched::UpdateRequired { running_version }) => {
                Ok(SshLaunch::UpdateRequired { running_version })
            }
            Err(error) => Err(launch_error(&error)),
        }
    }

    /// Ends the keeper; returns `stopped`, `notRunning`, or `stillRunning`.
    pub fn stop(&self, operation_id: String) -> Result<String, SshError> {
        let launcher = SshLauncher::new(
            self.release.clone(),
            logger(self.events.clone()),
            Box::new(|_| {}),
        );
        let outcome = launcher
            .stop(&mut self.transport(), &operation_id)
            .map_err(|error| launch_error(&error))?;
        Ok(match outcome {
            StopOutcome::Stopped => "stopped",
            StopOutcome::NotRunning => "notRunning",
            StopOutcome::StillRunning => "stillRunning",
        }
        .to_string())
    }

    /// Ends the forward and the session. Not `close`, which the Kotlin bindings
    /// already define to free the object.
    pub fn disconnect(&self) {
        if let Ok(mut forward) = self.forward.lock() {
            forward.take();
        }
        runtime().block_on(self.connector.close());
    }
}

impl SshComputer {
    fn transport(&self) -> EmbeddedSsh {
        EmbeddedSsh {
            runtime: runtime().clone(),
            connector: self.connector.clone(),
            log: logger(self.events.clone()),
        }
    }

    /// The verified binary from the cache, downloading the archive first when
    /// the cache lacks it.
    fn open_release(&self) -> Result<Box<dyn Read>, &'static str> {
        let binary = self
            .cache
            .join(self.release.version())
            .join("openaide-app-server");
        let expected = self.release.sha256();
        if archive::sha256_file(&binary).ok().as_deref() != Some(expected) {
            let path = self
                .events
                .release_archive()
                .map_err(|_| "download_failed")?;
            let file = File::open(&path).map_err(|_| "download_failed")?;
            let extracted = archive::extract_verified(file, &binary, expected);
            let _ = std::fs::remove_file(&path);
            extracted?;
        }
        File::open(&binary)
            .map(|file| Box::new(file) as Box<dyn Read>)
            .map_err(|_| "release_unreadable")
    }
}

fn launch_error(error: &LaunchError) -> SshError {
    SshError::new(error.stage().as_str(), error.class())
}

fn private_key(seed: &[u8]) -> Result<PrivateKey, SshError> {
    let seed: &[u8; 32] = seed
        .try_into()
        .map_err(|_| SshError::new("key", "key_invalid"))?;
    PrivateKey::new(KeypairData::Ed25519(Ed25519Keypair::from_seed(seed)), "")
        .map_err(|_| SshError::new("key", "key_invalid"))
}

fn parse_host_key(text: &str) -> Result<PublicKey, SshError> {
    PublicKey::from_openssh(text).map_err(|_| SshError::new("trust", "host_key_invalid"))
}

/// An OpenSSH public key line this library made: safe inside `'…'`.
fn is_key_line(line: &str) -> bool {
    line.starts_with("ssh-ed25519 ")
        && line.len() < 512
        && line
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || " +/=@._-".contains(character))
}

fn logger(events: Arc<dyn SshEvents>) -> Logger {
    Arc::new(move |event, fields| events.log(event.to_string(), fields.to_string()))
}

/// One runtime for the process: sessions, forwards, and their timers.
fn runtime() -> &'static Arc<Runtime> {
    static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("openaide-ssh")
                .enable_all()
                .build()
                .expect("the SSH runtime starts"),
        )
    })
}

#[cfg(test)]
mod tests;
