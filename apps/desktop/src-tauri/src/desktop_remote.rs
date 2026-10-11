//! Desktop as a Remote Device of an App Server on another computer (ADR-0063).
//!
//! The shell pairs by Pairing Code and then starts no App Server of its own:
//! the WebView talks to a loopback gateway that carries its requests to the
//! paired computer. Which computer is in use is a bootstrap preference, so a
//! change takes effect through the same restart as a runtime environment change.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use openaide_remote_client as remote;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use crate::desktop_runtime::{DesktopBootstrapPreferences, write_bootstrap_preferences};
use crate::{DesktopState, LocalHttpConnection};

/// A computer whose App Server trusts this Desktop: its public key and the name
/// it gave itself. Neither is a credential; the device key is stored apart.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RemoteComputer {
    pub(crate) id: String,
    pub(crate) name: String,
}

impl DesktopBootstrapPreferences {
    /// The paired computer in use instead of this one.
    pub(crate) fn remote(&self) -> Option<&RemoteComputer> {
        let id = self.remote_computer.as_ref()?;
        self.remote_computers
            .iter()
            .find(|computer| &computer.id == id)
    }

    /// Remembers a computer, replacing an earlier pairing with it, and selects it.
    fn pair(&mut self, computer: RemoteComputer) {
        self.remote_computers
            .retain(|known| known.id != computer.id);
        self.remote_computer = Some(computer.id.clone());
        self.remote_computers.push(computer);
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
}

/// This Desktop's endpoint, bound on first use and kept for the process: a
/// second endpoint with the same key would compete with the first for its relay.
pub(crate) struct DesktopRemoteState {
    endpoint: tokio::sync::Mutex<Option<remote::Endpoint>>,
    key_path: PathBuf,
    failure_reported: AtomicBool,
}

impl DesktopRemoteState {
    pub(crate) fn new(app_local_data: &Path) -> Self {
        Self {
            endpoint: tokio::sync::Mutex::new(None),
            key_path: app_local_data.join("device.key"),
            failure_reported: AtomicBool::new(false),
        }
    }

    async fn endpoint(&self) -> Result<remote::Endpoint, String> {
        let mut bound = self.endpoint.lock().await;
        if let Some(endpoint) = bound.as_ref() {
            return Ok(endpoint.clone());
        }
        let key = device_key(&self.key_path)?;
        let started_at = Instant::now();
        eprintln!("desktop_remote_endpoint_bind_started attempt_count=1");
        let result = remote::bind(key).await;
        eprintln!(
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

/// Opens the gateway to the selected computer. The computer is reached first, so
/// startup can name the problem instead of showing an empty workspace.
pub(crate) async fn connect(
    app: &tauri::AppHandle,
    computer: &RemoteComputer,
) -> Result<LocalHttpConnection, String> {
    let server = computer
        .id
        .parse::<remote::PublicKey>()
        .map_err(|_| "The saved pairing is damaged. Pair with the computer again.".to_string())?;
    let endpoint = app.state::<DesktopRemoteState>().endpoint().await?;
    let client = Arc::new(remote::RemoteClient::new(endpoint, server, logger()));
    client
        .connection()
        .await
        .map_err(|failure| message(failure, &computer.name))?;
    let reporter = app.clone();
    let gateway = remote::Gateway::start(client, logger(), move |failure| {
        report_failure(&reporter, failure)
    })
    .await
    .map_err(|_| "OpenAIDE could not start its connection on this computer.".to_string())?;
    Ok(LocalHttpConnection {
        kind: "localHttp".to_string(),
        endpoint_url: format!("http://{}/probe", gateway.address),
        auth_token: gateway.token,
    })
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
        })
        .collect())
}

/// Spends the invite another computer shows, then restarts into its workspace.
#[tauri::command]
pub(crate) async fn desktop_remote_pair(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    remote_state: tauri::State<'_, DesktopRemoteState>,
    code: String,
) -> Result<(), String> {
    let invite = remote::InviteCode::decode(&code).map_err(|error| {
        match error {
            remote::PairingCodeError::WrongKind => {
                "That code adds a device to a computer. On the other computer, open Settings → Devices and choose Show code."
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
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    preferences.pair(RemoteComputer {
        id: paired.id.to_string(),
        name: if paired.name.is_empty() {
            "Computer".to_string()
        } else {
            paired.name
        },
    });
    write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    drop(preferences);
    eprintln!("desktop_remote_computer_selected reason=paired");
    crate::restart_desktop(&app);
    Ok(())
}

/// Chooses a paired computer, or this computer when `id` is absent.
#[tauri::command]
pub(crate) fn desktop_remote_use(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    id: Option<String>,
) -> Result<(), String> {
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    if let Some(id) = &id {
        if !preferences
            .remote_computers
            .iter()
            .any(|computer| &computer.id == id)
        {
            return Err("This computer is no longer paired.".to_string());
        }
    }
    if preferences.remote_computer == id {
        return Ok(());
    }
    eprintln!(
        "desktop_remote_computer_selected reason={}",
        if id.is_some() { "chosen" } else { "local" }
    );
    preferences.remote_computer = id;
    write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    drop(preferences);
    crate::restart_desktop(&app);
    Ok(())
}

/// Forgets a pairing on this side. The other computer keeps listing this one
/// until it is removed there; its key alone opens nothing here.
#[tauri::command]
pub(crate) fn desktop_remote_forget(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    let was_current = preferences.forget(&id);
    write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    drop(preferences);
    eprintln!("desktop_remote_computer_forgotten was_current={was_current}");
    if was_current {
        crate::restart_desktop(&app);
    }
    Ok(())
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
    eprintln!("desktop_remote_computer_selected reason=recovered");
    Ok(true)
}

/// Says once per run that retrying cannot help, and offers the way back.
fn report_failure(app: &tauri::AppHandle, failure: remote::Failure) {
    if app
        .state::<DesktopRemoteState>()
        .failure_reported
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    eprintln!(
        "desktop_remote_failure_reported error_class={}",
        failure.class()
    );
    let name = app
        .state::<DesktopState>()
        .preferences
        .lock()
        .ok()
        .and_then(|preferences| preferences.remote().map(|computer| computer.name.clone()))
        .unwrap_or_else(|| "The other computer".to_string());
    let handle = app.clone();
    app.dialog()
        .message(message(failure, &name))
        .title("OpenAIDE")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Use this computer".to_string(),
            "Keep trying".to_string(),
        ))
        .show(move |use_local| {
            if use_local && use_this_computer(&handle.state::<DesktopState>()).is_ok() {
                crate::restart_desktop(&handle);
            }
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
fn logger() -> remote::Logger {
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
        eprintln!("desktop_{event}{fields}");
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
    eprintln!("desktop_remote_device_key_created");
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
