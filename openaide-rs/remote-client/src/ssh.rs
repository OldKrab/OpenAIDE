//! Starting the App Server on an SSH Computer (ADR-0069), shared by the App
//! Shells that offer one.
//!
//! The App Shell installs its own release of the App Server on the computer,
//! asks the installed binary's keeper to attach to or start the App Server for
//! the default state root, and forwards the App Server's loopback endpoint to
//! its own loopback. The App Shell then holds the token like a same-machine App
//! Shell; nothing is paired. Each App Shell brings its own [`SshTransport`]: the
//! Desktop drives the system `ssh`, Android an embedded client.
//!
//! Every call blocks; App Shells run it off their UI thread.

mod remote;
mod version;

use std::io::Read;
use std::net::SocketAddr;
use std::time::Instant;

use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION;
use serde_json::json;

use crate::Logger;
use remote::{KeeperReport, Probe, RemoteEndpoint};
use version::Running;

/// One SSH connection to the computer, as the App Shell implements it.
pub trait SshTransport {
    /// Kept by the App Shell while it uses the App Server; dropping it ends the forward.
    type Forward;

    /// Runs one command line on the computer, streaming `input` to its stdin.
    /// A command that runs and fails is an `Ok` output; `Err` means SSH itself failed.
    fn run(
        &mut self,
        command: &str,
        input: Option<&mut dyn Read>,
    ) -> Result<RemoteOutput, TransportError>;

    /// Forwards a port on this device's loopback to `remote_port` on the
    /// computer's loopback and returns the local address.
    fn forward(&mut self, remote_port: u16) -> Result<(Self::Forward, SocketAddr), TransportError>;
}

pub struct RemoteOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
}

/// Why SSH failed, as a stable class the transport chooses, such as
/// `unreachable`, `auth_failed`, or `host_key_rejected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("ssh failed: {class}")]
pub struct TransportError {
    pub class: &'static str,
}

/// The App Server release this App Shell ships: its version and the SHA-256 of
/// its Linux x64 binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRelease {
    version: String,
    sha256: String,
}

impl ServerRelease {
    /// `None` unless the version is a plain release version and the checksum is
    /// lowercase hex, because both become words of a remote command.
    pub fn new(version: &str, sha256: &str) -> Option<Self> {
        (remote::is_version(version) && remote::is_sha256(sha256)).then(|| Self {
            version: version.to_string(),
            sha256: sha256.to_string(),
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Supplies the release binary only when the computer lacks it, so a first
/// download happens once and only when needed. Errors are stable classes.
pub type FetchBinary<'a> = &'a mut dyn FnMut() -> Result<Box<dyn Read>, &'static str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartMode {
    /// Attach to the App Server already there, or start one; never replace one.
    Keep,
    /// Replace an older App Server. Only after the user confirmed the update.
    Replace,
}

/// A step the startup screen names while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Probe,
    Fetch,
    Upload,
    Start,
    Forward,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Fetch => "fetch",
            Self::Upload => "upload",
            Self::Start => "start",
            Self::Forward => "forward",
        }
    }
}

pub enum Launched<F> {
    Ready(Ready<F>),
    /// An older App Server this App Shell cannot use holds the state root.
    /// Launching again with [`StartMode::Replace`] updates it once the user
    /// agrees to interrupt whatever it is doing.
    UpdateRequired {
        running_version: String,
    },
}

/// An App Server this App Shell can use through the forward. Not `Debug`: it
/// holds the token.
pub struct Ready<F> {
    pub forward: F,
    pub endpoint_url: String,
    pub auth_token: String,
    pub app_version: String,
    /// The App Server is older than this App Shell but speaks its protocol, so
    /// the App Shell attached and may offer an update.
    pub update_available: bool,
    /// Another App Shell's keeper already held the App Server.
    pub keeper_existed: bool,
    /// Logind will end the App Server when the user's last session closes;
    /// `loginctl enable-linger` keeps it.
    pub suggest_linger: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    #[error("ssh failed during {}: {}", .stage.as_str(), .error.class)]
    Transport { stage: Stage, error: TransportError },
    #[error("the computer is not Linux x64")]
    UnsupportedPlatform,
    #[error("the release binary is unavailable: {0}")]
    Fetch(&'static str),
    #[error("the release binary could not be installed: {0}")]
    Install(&'static str),
    #[error("the computer runs a newer App Server ({running_version})")]
    ServerNewer { running_version: String },
    #[error("the computer runs an App Server of an unknown version")]
    ServerVersionUnknown,
    #[error("the keeper failed: {0}")]
    Keeper(String),
    #[error("the computer gave an unreadable answer during {}", .0.as_str())]
    UnreadableAnswer(Stage),
}

impl LaunchError {
    pub fn stage(&self) -> Stage {
        match self {
            Self::Transport { stage, .. } | Self::UnreadableAnswer(stage) => *stage,
            Self::UnsupportedPlatform => Stage::Probe,
            Self::Fetch(_) => Stage::Fetch,
            Self::Install(_) => Stage::Upload,
            Self::ServerNewer { .. } | Self::ServerVersionUnknown | Self::Keeper(_) => Stage::Start,
        }
    }

    pub fn class(&self) -> &str {
        match self {
            Self::Transport { error, .. } => error.class,
            Self::UnsupportedPlatform => "unsupported_platform",
            Self::Fetch(class) | Self::Install(class) => class,
            Self::ServerNewer { .. } => "server_newer",
            Self::ServerVersionUnknown => "server_version_unknown",
            Self::Keeper(class) => class,
            Self::UnreadableAnswer(_) => "unreadable_answer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    Stopped,
    NotRunning,
    /// The keeper did not end in time; the App Server keeps running for now.
    StillRunning,
}

/// Starts App Servers on SSH Computers for one App Shell release.
pub struct SshLauncher {
    release: ServerRelease,
    logger: Logger,
    progress: Box<dyn Fn(Stage) + Send + Sync>,
}

impl SshLauncher {
    pub fn new(
        release: ServerRelease,
        logger: Logger,
        progress: Box<dyn Fn(Stage) + Send + Sync>,
    ) -> Self {
        Self {
            release,
            logger,
            progress,
        }
    }

    /// Installs this release if the computer lacks it, has its keeper hold an
    /// App Server, and forwards it. `operation_id` correlates the log events.
    pub fn launch<T: SshTransport>(
        &self,
        transport: &mut T,
        mode: StartMode,
        fetch: FetchBinary<'_>,
        operation_id: &str,
    ) -> Result<Launched<T::Forward>, LaunchError> {
        let started = Instant::now();
        (self.logger)(
            "ssh_launch_started",
            json!({ "operationId": operation_id, "mode": mode_name(mode) }),
        );
        let result = self.launch_stages(transport, mode, fetch, operation_id);
        let mut fields = json!({
            "operationId": operation_id,
            "durationMs": started.elapsed().as_millis() as u64,
        });
        match &result {
            Ok(Launched::Ready(ready)) => {
                fields["outcome"] = json!("ready");
                fields["keeper"] = json!(if ready.keeper_existed {
                    "existing"
                } else {
                    "new"
                });
                fields["updateAvailable"] = json!(ready.update_available);
                fields["suggestLinger"] = json!(ready.suggest_linger);
            }
            Ok(Launched::UpdateRequired { .. }) => fields["outcome"] = json!("update_required"),
            Err(error) => {
                fields["outcome"] = json!("failure");
                fields["stage"] = json!(error.stage().as_str());
                fields["errorClass"] = json!(error.class());
            }
        }
        (self.logger)("ssh_launch_completed", fields);
        result
    }

    fn launch_stages<T: SshTransport>(
        &self,
        transport: &mut T,
        mode: StartMode,
        fetch: FetchBinary<'_>,
        operation_id: &str,
    ) -> Result<Launched<T::Forward>, LaunchError> {
        let version = self.release.version.as_str();
        let probe = self.stage(Stage::Probe, operation_id, || {
            let output = run(transport, Stage::Probe, &remote::probe(version), None)?;
            let probe = Probe::parse(&output.stdout);
            if !probe.is_supported() {
                return Err(LaunchError::UnsupportedPlatform);
            }
            Ok(probe)
        })?;
        if probe.installed_sha256.as_deref() != Some(self.release.sha256.as_str()) {
            let mut binary = self.stage(Stage::Fetch, operation_id, || {
                fetch().map_err(LaunchError::Fetch)
            })?;
            self.stage(Stage::Upload, operation_id, || {
                let command = remote::install(version, &self.release.sha256);
                let output = run(transport, Stage::Upload, &command, Some(&mut *binary))?;
                remote::install_verdict(&output.stdout).map_err(LaunchError::Install)
            })?;
        }

        let started = self.stage(Stage::Start, operation_id, || {
            let flags: &[&str] = match mode {
                StartMode::Keep => &["start"],
                StartMode::Replace => &["start", "--replace"],
            };
            let report = keep(transport, version, flags)?;
            if report.outcome != "mismatch" {
                return accept(report, false);
            }
            let running_version = report.app_version.clone().unwrap_or_default();
            match version::classify(
                version,
                APP_SERVER_PROTOCOL_VERSION,
                &running_version,
                report.protocol_version.as_deref(),
            ) {
                Running::Newer => Err(LaunchError::ServerNewer { running_version }),
                Running::Unknown => Err(LaunchError::ServerVersionUnknown),
                Running::OlderIncompatible => Ok(Start::UpdateRequired { running_version }),
                Running::OlderCompatible => {
                    accept(keep(transport, version, &["start", "--join"])?, true)
                }
            }
        })?;
        let held = match started {
            Start::Held(held) => held,
            Start::UpdateRequired { running_version } => {
                return Ok(Launched::UpdateRequired { running_version })
            }
        };

        let (forward, local) = self.stage(Stage::Forward, operation_id, || {
            transport
                .forward(held.endpoint.port)
                .map_err(|error| LaunchError::Transport {
                    stage: Stage::Forward,
                    error,
                })
        })?;
        Ok(Launched::Ready(Ready {
            forward,
            endpoint_url: format!("http://{local}{}", held.endpoint.path),
            auth_token: held.auth_token,
            app_version: held.app_version,
            update_available: held.joined_older,
            keeper_existed: held.keeper_existed,
            suggest_linger: probe.suggests_linger(),
        }))
    }

    /// Lets the App Server go: its keeper ends, and the App Server stops once
    /// its last App Shell leaves too.
    pub fn stop<T: SshTransport>(
        &self,
        transport: &mut T,
        operation_id: &str,
    ) -> Result<StopOutcome, LaunchError> {
        let started = Instant::now();
        let result = keep(transport, &self.release.version, &["stop"]).and_then(|report| {
            match report.outcome.as_str() {
                "stopped" => Ok(StopOutcome::Stopped),
                "not_running" => Ok(StopOutcome::NotRunning),
                "still_running" => Ok(StopOutcome::StillRunning),
                _ => Err(LaunchError::Keeper(report.error_class())),
            }
        });
        (self.logger)(
            "ssh_stop_completed",
            json!({
                "operationId": operation_id,
                "outcome": match &result {
                    Ok(StopOutcome::Stopped) => "stopped",
                    Ok(StopOutcome::NotRunning) => "not_running",
                    Ok(StopOutcome::StillRunning) => "still_running",
                    Err(_) => "failure",
                },
                "errorClass": result.as_ref().err().map(LaunchError::class),
                "durationMs": started.elapsed().as_millis() as u64,
            }),
        );
        result
    }

    /// Announces a stage to the startup screen and logs how it ended.
    fn stage<R>(
        &self,
        stage: Stage,
        operation_id: &str,
        body: impl FnOnce() -> Result<R, LaunchError>,
    ) -> Result<R, LaunchError> {
        (self.progress)(stage);
        let started = Instant::now();
        let result = body();
        (self.logger)(
            "ssh_launch_stage_completed",
            json!({
                "operationId": operation_id,
                "stage": stage.as_str(),
                "outcome": if result.is_ok() { "success" } else { "failure" },
                "errorClass": result.as_ref().err().map(LaunchError::class),
                "durationMs": started.elapsed().as_millis() as u64,
            }),
        );
        result
    }
}

enum Start {
    Held(Held),
    UpdateRequired { running_version: String },
}

/// The App Server a keeper holds, before it is forwarded.
struct Held {
    endpoint: RemoteEndpoint,
    auth_token: String,
    app_version: String,
    keeper_existed: bool,
    joined_older: bool,
}

fn accept(report: KeeperReport, joined_older: bool) -> Result<Start, LaunchError> {
    if !matches!(report.outcome.as_str(), "started" | "attached" | "joined") {
        return Err(LaunchError::Keeper(report.error_class()));
    }
    let unreadable = || LaunchError::UnreadableAnswer(Stage::Start);
    let endpoint = report
        .endpoint_url
        .as_deref()
        .and_then(RemoteEndpoint::parse)
        .ok_or_else(unreadable)?;
    Ok(Start::Held(Held {
        endpoint,
        auth_token: report
            .auth_token
            .filter(|token| !token.is_empty())
            .ok_or_else(unreadable)?,
        app_version: report.app_version.unwrap_or_default(),
        keeper_existed: report.keeper.as_deref() == Some("existing"),
        joined_older,
    }))
}

fn keep<T: SshTransport>(
    transport: &mut T,
    version: &str,
    arguments: &[&str],
) -> Result<KeeperReport, LaunchError> {
    let output = run(
        transport,
        Stage::Start,
        &remote::keep(version, arguments),
        None,
    )?;
    KeeperReport::parse(&output.stdout).ok_or(LaunchError::UnreadableAnswer(Stage::Start))
}

fn run<T: SshTransport>(
    transport: &mut T,
    stage: Stage,
    command: &str,
    input: Option<&mut dyn Read>,
) -> Result<RemoteOutput, LaunchError> {
    transport
        .run(command, input)
        .map_err(|error| LaunchError::Transport { stage, error })
}

fn mode_name(mode: StartMode) -> &'static str {
    match mode {
        StartMode::Keep => "keep",
        StartMode::Replace => "replace",
    }
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
