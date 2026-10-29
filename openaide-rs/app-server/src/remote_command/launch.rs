//! Finds the App Server for this state root, or starts one that outlives the
//! terminal the command runs in.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use openaide_app_server::storage_runtime::StateRoot;

use crate::app_server_handoff::{self, HandoffStart, ReportingHandoff};

/// Where the App Server answers same-machine clients.
pub(crate) struct Connection {
    pub(crate) endpoint_url: String,
    pub(crate) auth_token: String,
    /// True when this command started the App Server rather than joining one.
    pub(crate) started: bool,
    /// Held so the launcher's output pipe stays open for as long as the command runs.
    _launcher: Option<Child>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchError {
    Spawn,
    NoConnection,
}

impl LaunchError {
    pub(crate) fn class(self) -> &'static str {
        match self {
            Self::Spawn => "spawn_failed",
            Self::NoConnection => "no_connection",
        }
    }
}

/// What a command that neither starts nor replaces an App Server finds.
pub(crate) enum Found {
    Joined(Connection),
    NotRunning,
    /// A live App Server of another version, left running.
    OtherVersion {
        protocol_version: String,
    },
}

/// Joins the running App Server without starting or replacing one: a short
/// command run with another binary must not stop the App Server a service keeps.
pub(crate) fn running(state_root: &StateRoot, runtime_root: &Path) -> Result<Found, &'static str> {
    running_from(app_server_handoff::attach_or_launch_reporting(
        state_root,
        runtime_root,
    ))
}

fn running_from(handoff: Result<ReportingHandoff, &'static str>) -> Result<Found, &'static str> {
    Ok(match handoff? {
        ReportingHandoff::Attached(connection) => Found::Joined(joined(&connection)),
        // Dropping the launch lock leaves starting to whoever asks for it.
        ReportingHandoff::LaunchHere(_lock) => Found::NotRunning,
        ReportingHandoff::Incompatible(server) => Found::OtherVersion {
            protocol_version: server.protocol_version,
        },
    })
}

/// Joins the running App Server without starting one. One of another version
/// is replaced, as when an App Shell of this version starts.
fn attach(state_root: &StateRoot, runtime_root: &Path) -> Option<Connection> {
    match app_server_handoff::attach_or_launch(state_root, runtime_root) {
        HandoffStart::Attached(connection) => Some(joined(&connection)),
        // Dropping the launch lock leaves starting to whoever asks for it.
        HandoffStart::LaunchHere(_lock) => None,
    }
}

fn joined(connection: &app_server_handoff::LocalHttpHandoffConnection) -> Connection {
    Connection {
        endpoint_url: connection.endpoint_url().to_string(),
        auth_token: connection.auth_token().to_string(),
        started: false,
        _launcher: None,
    }
}

/// Joins the running App Server or starts one the way an App Shell does: a
/// separate process that prints its address and then lives by its own clients.
pub(crate) fn attach_or_start(
    state_root: &StateRoot,
    runtime_root: &Path,
) -> Result<Connection, LaunchError> {
    if let Some(connection) = attach(state_root, runtime_root) {
        return Ok(connection);
    }
    start()
}

/// Starts an App Server for the default state root in a separate process and reads
/// the address it prints. If another one won the launch meanwhile, the started
/// process prints that one's address instead and exits.
pub(crate) fn start() -> Result<Connection, LaunchError> {
    let executable = std::env::current_exe().map_err(|_| LaunchError::Spawn)?;
    let mut command = Command::new(executable);
    command
        // This App Server again, so it keeps this instance's settings.
        .envs(openaide_app_server::instance_env::for_own_launch())
        .env(
            crate::APP_SERVER_PROTOCOL_MODE_ENV,
            crate::APP_SERVER_HANDOFF_MODE,
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // The App Server writes its diagnostics to its log file; a closed
        // terminal must not become a failed write.
        .stderr(Stdio::null());
    detach_from_terminal(&mut command);
    let mut launcher = command.spawn().map_err(|_| LaunchError::Spawn)?;
    let stdout = launcher.stdout.take().ok_or(LaunchError::NoConnection)?;
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .map_err(|_| LaunchError::NoConnection)?;
    let value: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|_| LaunchError::NoConnection)?;
    let endpoint_url = value["endpointUrl"]
        .as_str()
        .ok_or(LaunchError::NoConnection)?;
    let auth_token = value["authToken"]
        .as_str()
        .ok_or(LaunchError::NoConnection)?;
    Ok(Connection {
        endpoint_url: endpoint_url.to_string(),
        auth_token: auth_token.to_string(),
        started: true,
        _launcher: Some(launcher),
    })
}

/// Ctrl+C in the terminal reaches only the command. The App Server stops by
/// its own rule, after its last same-machine client leaves.
#[cfg(unix)]
fn detach_from_terminal(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach_from_terminal(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
