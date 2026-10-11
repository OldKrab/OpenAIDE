//! Finds the App Server for this state root, or starts one that outlives the
//! terminal the command runs in.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use openaide_app_server::storage_runtime::StateRoot;

use crate::app_server_handoff::{self, HandoffStart};

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

/// Joins the running App Server without starting one.
pub(crate) fn attach(state_root: &StateRoot, runtime_root: &Path) -> Option<Connection> {
    match app_server_handoff::attach_or_launch(state_root, runtime_root) {
        HandoffStart::Attached(connection) => Some(Connection {
            endpoint_url: connection.endpoint_url().to_string(),
            auth_token: connection.auth_token().to_string(),
            started: false,
            _launcher: None,
        }),
        // Dropping the launch lock leaves starting to whoever asks for it.
        HandoffStart::LaunchHere(_lock) => None,
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
    let executable = std::env::current_exe().map_err(|_| LaunchError::Spawn)?;
    let mut command = Command::new(executable);
    command
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
