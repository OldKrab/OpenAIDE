//! Desktop as a Remote Device of an App Server on another computer (ADR-0063).
//!
//! The shell pairs by Pairing Code. While a paired computer is in use, the
//! WebView talks to a loopback gateway that carries its requests there, and this
//! computer's App Server keeps running (ADR-0070). A choice takes effect in place
//! through [`crate::desktop_connection`]; it is saved for the next start once the
//! computer answered.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use openaide_remote_client as remote;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use crate::DesktopState;
use crate::desktop_app_server_launch::LocalHttpConnection;
use crate::desktop_connection::{
    ConnectError, DesktopConnections, SwitchError, SwitchOutcome, ensure_local, reload_window,
};
use crate::desktop_runtime::{DesktopBootstrapPreferences, write_bootstrap_preferences};

/// A computer whose App Server this Desktop may use instead of its own. A
/// paired computer is known by its public key and the name it gave itself; an
/// SSH Computer (ADR-0069) by its `ssh` destination. None is a credential; the
/// device key is stored apart.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RemoteComputer {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ssh: Option<String>,
}

impl DesktopBootstrapPreferences {
    /// The paired or SSH Computer in use instead of this one.
    pub(crate) fn remote(&self) -> Option<&RemoteComputer> {
        let id = self.remote_computer.as_ref()?;
        self.remote_computers
            .iter()
            .find(|computer| &computer.id == id)
    }

    /// Remembers a computer, replacing an earlier pairing with it. Using it is a
    /// switch of its own, saved once the computer answers.
    pub(crate) fn remember(&mut self, computer: RemoteComputer) {
        self.remote_computers
            .retain(|known| known.id != computer.id);
        self.remote_computers.push(computer);
    }

    fn paired(&self, id: &str) -> Option<&RemoteComputer> {
        self.remote_computers
            .iter()
            .find(|computer| computer.id == id)
    }

    /// Forgets a computer; true when it was the one in use.
    fn forget(&mut self, id: &str) -> bool {
        self.remote_computers.retain(|known| known.id != id);
        let was_current = self.remote_computer.as_deref() == Some(id);
        if was_current {
            self.remote_computer = None;
        }
        was_current
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteComputerSummary {
    id: String,
    name: String,
    current: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh: Option<String>,
}

/// This Desktop's endpoint, bound on first use and kept for the process: a
/// second endpoint with the same key would compete with the first for its relay.
pub(crate) struct DesktopRemoteState {
    endpoint: tokio::sync::Mutex<Option<remote::Endpoint>>,
    key_path: PathBuf,
}

impl DesktopRemoteState {
    pub(crate) fn new(app_local_data: &Path) -> Self {
        Self {
            endpoint: tokio::sync::Mutex::new(None),
            key_path: app_local_data.join("device.key"),
        }
    }

    async fn endpoint(&self) -> Result<remote::Endpoint, String> {
        let mut bound = self.endpoint.lock().await;
        if let Some(endpoint) = bound.as_ref() {
            return Ok(endpoint.clone());
        }
        let key = device_key(&self.key_path)?;
        let started_at = Instant::now();
        desktop_log!("desktop_remote_endpoint_bind_started attempt_count=1");
        let result = remote::bind(key, logger()).await;
        desktop_log!(
            "desktop_remote_endpoint_bind_completed outcome={} duration_ms={} attempt_count=1",
            if result.is_ok() { "success" } else { "failure" },
            started_at.elapsed().as_millis()
        );
        let endpoint = result
            .map_err(|_| "OpenAIDE could not start its connection on this computer.".to_string())?;
        *bound = Some(endpoint.clone());
        Ok(endpoint)
    }
}

/// Opens a gateway to `computer`. The computer is reached first, so a switch or
/// startup can name the problem instead of showing an empty workspace.
pub(crate) async fn connect(
    app: &tauri::AppHandle,
    computer: &RemoteComputer,
) -> Result<(LocalHttpConnection, remote::Gateway), ConnectError> {
    let server = computer
        .id
        .parse::<remote::PublicKey>()
        .map_err(|_| ConnectError {
            class: "pairing_damaged",
            message: "The saved pairing is damaged. Pair with the computer again.".to_string(),
        })?;
    let this_computer = |message: String| ConnectError {
        class: "endpoint",
        message,
    };
    let endpoint = app
        .state::<DesktopRemoteState>()
        .endpoint()
        .await
        .map_err(this_computer)?;
    let client = Arc::new(remote::RemoteClient::new(endpoint, server, logger()));
    client.connection().await.map_err(|failure| ConnectError {
        class: failure.class(),
        message: message(failure, &computer.name),
    })?;
    let reporter = app.clone();
    let name = computer.name.clone();
    // Each connection says once that retrying cannot help, so a later switch to
    // the same or another computer can be told again.
    let reported = AtomicBool::new(false);
    let gateway = remote::Gateway::start(client, logger(), move |failure| {
        if !reported.swap(true, Ordering::AcqRel) {
            report_failure(&reporter, &name, failure);
        }
    })
    .await
    .map_err(|_| {
        this_computer("OpenAIDE could not start its connection on this computer.".to_string())
    })?;
    let connection = LocalHttpConnection {
        kind: "localHttp".to_string(),
        endpoint_url: format!("http://{}/probe", gateway.address),
        auth_token: gateway.token.clone(),
    };
    Ok((connection, gateway))
}

/// Moves the window to a paired computer, or to this computer for `None`, once
/// the target answers. Saving the choice is the last step, so a failure changes
/// nothing.
pub(crate) async fn switch(
    app: &tauri::AppHandle,
    target: Option<String>,
    operation_id: &str,
) -> Result<SwitchOutcome, SwitchError> {
    let state = app.state::<DesktopState>();
    let computer = match &target {
        Some(id) => Some(
            state
                .preferences
                .lock()
                .map_err(|_| {
                    SwitchError::Failed("Desktop preferences are unavailable.".to_string())
                })?
                .paired(id)
                .cloned()
                .ok_or_else(|| {
                    SwitchError::Failed("This computer is no longer paired.".to_string())
                })?,
        ),
        None => None,
    };
    let connections = app.state::<DesktopConnections>();
    connections
        .switch(
            target,
            operation_id,
            || async {
                match &computer {
                    Some(computer) => match &computer.ssh {
                        Some(destination) => {
                            crate::desktop_ssh::connect(app, computer, destination, false)
                                .await
                                .map(|connection| (connection, None))
                                .map_err(|message| ConnectError {
                                    class: "ssh_connect",
                                    message,
                                })
                        }
                        None => connect(app, computer)
                            .await
                            .map(|(connection, gateway)| (connection, Some(gateway))),
                    },
                    None => ensure_local(app, None, operation_id.to_string())
                        .await
                        .map(|connection| (connection, None))
                        .map_err(|message| ConnectError {
                            class: "app_server_start",
                            message,
                        }),
                }
            },
            |target| {
                let mut preferences = state
                    .preferences
                    .lock()
                    .map_err(|_| "Desktop preferences are unavailable.".to_string())?;
                preferences.remote_computer = target.clone();
                write_bootstrap_preferences(&state.preferences_path, &preferences)
            },
        )
        .await
}

/// What a switch the window asked for came to; the window reloads after `switched`.
fn outcome_name(result: Result<SwitchOutcome, SwitchError>) -> Result<&'static str, String> {
    match result {
        Ok(SwitchOutcome::Switched) => Ok("switched"),
        Ok(SwitchOutcome::AlreadyInUse) => Ok("alreadyInUse"),
        Err(SwitchError::Cancelled) => Ok("cancelled"),
        Err(SwitchError::Failed(message)) => Err(message),
    }
}

/// A correlation id from the window, used in logs only when it is plainly safe.
fn operation_id_or_new(candidate: Option<String>) -> String {
    candidate
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 80
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .unwrap_or_else(|| format!("desktop-computer-switch-{}", uuid::Uuid::new_v4()))
}

#[tauri::command]
pub(crate) fn desktop_remote_snapshot(
    state: tauri::State<'_, DesktopState>,
) -> Result<Vec<RemoteComputerSummary>, String> {
    let preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    Ok(preferences
        .remote_computers
        .iter()
        .map(|computer| RemoteComputerSummary {
            id: computer.id.clone(),
            name: computer.name.clone(),
            current: preferences.remote_computer.as_ref() == Some(&computer.id),
            ssh: computer.ssh.clone(),
        })
        .collect())
}

/// Spends the invite another computer shows, then switches to that computer.
#[tauri::command]
pub(crate) async fn desktop_remote_pair(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    remote_state: tauri::State<'_, DesktopRemoteState>,
    code: String,
    operation_id: Option<String>,
) -> Result<&'static str, String> {
    let invite = remote::InviteCode::decode(&code).map_err(|error| {
        match error {
            remote::PairingCodeError::WrongKind => {
                "That code adds a device to a computer. On the other computer, open Settings → Remote and choose Show code."
            }
            remote::PairingCodeError::Malformed => "The code is incomplete. Copy it again.",
            remote::PairingCodeError::NotAPairingCode => "That is not an OpenAIDE pairing code.",
        }
        .to_string()
    })?;
    let endpoint = remote_state.endpoint().await?;
    let paired = remote::redeem(
        &endpoint,
        &invite,
        &device_name(),
        Some(device_model()),
        &logger(),
    )
    .await
    .map_err(|failure| match failure {
        remote::Failure::NotTrusted => {
            "This code has expired or was already used. Show a new code on the other computer."
                .to_string()
        }
        other => message(other, "The other computer"),
    })?;
    let id = paired.id.to_string();
    {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|_| "desktop bootstrap preferences unavailable")?;
        preferences.remember(RemoteComputer {
            id: id.clone(),
            name: if paired.name.is_empty() {
                "Computer".to_string()
            } else {
                paired.name
            },
            ssh: None,
        });
        write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    }
    desktop_log!("desktop_remote_computer_paired");
    outcome_name(switch(&app, Some(id), &operation_id_or_new(operation_id)).await)
}

/// Switches the window to a paired computer, or to this computer when `id` is
/// absent: `switched` (the window reloads), `alreadyInUse`, or `cancelled`. A
/// computer that cannot be reached is an error and changes nothing.
#[tauri::command]
pub(crate) async fn desktop_remote_use(
    app: tauri::AppHandle,
    id: Option<String>,
    operation_id: Option<String>,
) -> Result<&'static str, String> {
    use_computer(&app, id, operation_id).await
}

/// [`desktop_remote_use`] for the other commands that end in a switch.
pub(crate) async fn use_computer(
    app: &tauri::AppHandle,
    id: Option<String>,
    operation_id: Option<String>,
) -> Result<&'static str, String> {
    outcome_name(switch(app, id, &operation_id_or_new(operation_id)).await)
}

/// Ends the switch attempt in progress; the window stays where it is.
#[tauri::command]
pub(crate) fn desktop_remote_cancel(connections: tauri::State<'_, DesktopConnections>) -> bool {
    let cancelled = connections.cancel();
    desktop_log!("desktop_computer_switch_cancel_requested pending={cancelled}");
    cancelled
}

/// Forgets a pairing on this side. Forgetting the computer in use first moves
/// the window to this computer. The other computer keeps listing this one until
/// it is removed there; its key alone opens nothing here.
#[tauri::command]
pub(crate) async fn desktop_remote_forget(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    connections: tauri::State<'_, DesktopConnections>,
    id: String,
    operation_id: Option<String>,
) -> Result<&'static str, String> {
    let in_use = connections.computer_in_use() == Some(Some(id.clone()));
    let outcome = if in_use {
        outcome_name(switch(&app, None, &operation_id_or_new(operation_id)).await)?
    } else {
        "alreadyInUse"
    };
    if outcome == "cancelled" {
        return Ok(outcome);
    }
    {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|_| "desktop bootstrap preferences unavailable")?;
        preferences.forget(&id);
        write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    }
    desktop_log!("desktop_remote_computer_forgotten was_in_use={in_use}");
    Ok(outcome)
}

/// Returns to this computer's own App Server on the next start.
pub(crate) fn use_this_computer(state: &DesktopState) -> Result<bool, String> {
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    if preferences.remote_computer.take().is_none() {
        return Ok(false);
    }
    write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    desktop_log!("desktop_remote_computer_selected reason=recovered");
    Ok(true)
}

/// Says that retrying cannot help, and offers the way back to this computer.
fn report_failure(app: &tauri::AppHandle, name: &str, failure: remote::Failure) {
    desktop_log!(
        "desktop_remote_failure_reported error_class={}",
        failure.class()
    );
    let handle = app.clone();
    app.dialog()
        .message(message(failure, name))
        .title("OpenAIDE")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Use this computer".to_string(),
            "Keep trying".to_string(),
        ))
        .show(move |use_local| {
            if !use_local {
                return;
            }
            tauri::async_runtime::spawn(async move {
                let operation_id = operation_id_or_new(None);
                if switch(&handle, None, &operation_id).await == Ok(SwitchOutcome::Switched) {
                    reload_window(&handle);
                }
            });
        });
}

fn message(failure: remote::Failure, name: &str) -> String {
    match failure {
        remote::Failure::Unreachable => format!(
            "{name} is unreachable. Make sure OpenAIDE is open on it and both computers are online."
        ),
        remote::Failure::NotTrusted => {
            format!("This computer was removed from {name}. Pair with it again in Settings.")
        }
        remote::Failure::Incompatible => format!(
            "OpenAIDE on this computer and on {name} are different versions. Update both, then try again."
        ),
    }
}

/// Prints the client's metadata-only events in this shell's log format.
pub(crate) fn logger() -> remote::Logger {
    Arc::new(|event, fields| {
        let fields = fields
            .as_object()
            .map(|fields| {
                fields
                    .iter()
                    .map(|(name, value)| match value.as_str() {
                        Some(text) => format!(" {name}={text}"),
                        None => format!(" {name}={value}"),
                    })
                    .collect::<String>()
            })
            .unwrap_or_default();
        desktop_log!("desktop_{event}{fields}");
    })
}

/// The name the other computer lists this one under.
fn device_name() -> String {
    #[cfg(target_os = "macos")]
    let friendly = command_line("scutil", &["--get", "ComputerName"]);
    #[cfg(not(target_os = "macos"))]
    let friendly = None;
    friendly
        .or_else(|| command_line("hostname", &[]))
        .map(|name| remote::clamp_label(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Computer".to_string())
}

fn device_model() -> &'static str {
    match std::env::consts::OS {
        "macos" => "OpenAIDE Desktop on macOS",
        "windows" => "OpenAIDE Desktop on Windows",
        _ => "OpenAIDE Desktop",
    }
}

fn command_line(program: &str, arguments: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(program);
    command.args(arguments);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// This Desktop's key pair, created on first use. It is the only secret of a
/// pairing: whoever holds it is this device to every computer that trusts it.
fn device_key(path: &Path) -> Result<remote::SecretKey, String> {
    let unavailable =
        || "Secure storage is unavailable. Restart OpenAIDE and try again.".to_string();
    if let Some(text) = read_device_key(path).map_err(|()| unavailable())? {
        if let Some(key) = remote::device_key_from_text(&text) {
            return Ok(key);
        }
    }
    let key = remote::new_device_key();
    write_device_key(path, &remote::device_key_text(&key)).map_err(|()| unavailable())?;
    desktop_log!("desktop_remote_device_key_created");
    Ok(key)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn device_key_entry() -> Result<keyring::Entry, ()> {
    keyring::Entry::new("dev.openaide.desktop.device", "device-key").map_err(|_| ())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn read_device_key(_path: &Path) -> Result<Option<String>, ()> {
    match device_key_entry()?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(()),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn write_device_key(_path: &Path, text: &str) -> Result<(), ()> {
    device_key_entry()?.set_password(text).map_err(|_| ())
}

// Other platforms have no credential store wired into this shell, so the key is
// a file only its owner can read, next to the rest of the shell's own data.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_device_key(path: &Path) -> Result<Option<String>, ()> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn write_device_key(path: &Path, text: &str) -> Result<(), ()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ())?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|_| ())
}

#[cfg(test)]
#[path = "desktop_remote_tests.rs"]
mod tests;
