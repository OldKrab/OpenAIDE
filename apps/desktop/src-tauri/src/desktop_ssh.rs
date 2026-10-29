//! SSH Computers in the Desktop (ADR-0069).
//!
//! An SSH Computer is one more choice of where agents work, kept beside paired
//! computers and switched through the same restart. Starting with one installs
//! this Desktop's App Server there through the system `ssh`, has its keeper hold
//! it, and forwards its endpoint to loopback; the WebView then talks to it like
//! to this computer's own App Server. This module owns the forward for the
//! process and what the WebView may ask about it: the last outcome, prompts,
//! updates, and stopping.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use openaide_remote_client::{LaunchError, Launched, SshLauncher, Stage, StartMode, StopOutcome};
use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::DesktopState;
use crate::desktop_app_server_launch::{LocalHttpConnection, app_server_endpoint_reachable};
use crate::desktop_connection::DesktopConnections;
use crate::desktop_remote::RemoteComputer;
use crate::desktop_runtime::write_bootstrap_preferences;
use crate::desktop_ssh_askpass::{AskpassBroker, SshPrompt};
use crate::desktop_ssh_release::ReleaseSource;
use crate::desktop_ssh_transport::{SshForward, SystemSsh, is_destination};

pub(crate) struct DesktopSshState {
    askpass: Mutex<Option<Arc<AskpassBroker>>>,
    forward: Mutex<Option<SshForward>>,
    status: Mutex<Option<SshStatus>>,
    failure: Mutex<Option<SshFailure>>,
    cache_root: PathBuf,
}

/// What the App Shell knows about the SSH Computer it uses.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SshStatus {
    computer_id: String,
    running_version: String,
    /// The App Server there is older but compatible; an update may be offered.
    update_available: bool,
    /// Logind would end the App Server when the user's last session closes.
    suggest_linger: bool,
}

/// Why the last start did not reach the SSH Computer's App Server.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SshFailure {
    computer_id: String,
    /// `updateRequired` when only a confirmed update can help.
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    running_version: Option<String>,
    stage: &'static str,
    error_class: String,
}

impl DesktopSshState {
    pub(crate) fn new(cache_root: PathBuf) -> Self {
        Self {
            askpass: Mutex::new(None),
            forward: Mutex::new(None),
            status: Mutex::new(None),
            failure: Mutex::new(None),
            cache_root,
        }
    }

    fn askpass(&self, app: &tauri::AppHandle) -> Result<Arc<AskpassBroker>, String> {
        let mut broker = self
            .askpass
            .lock()
            .map_err(|_| "SSH state is unavailable.".to_string())?;
        if let Some(broker) = broker.as_ref() {
            return Ok(broker.clone());
        }
        let started = AskpassBroker::start(app.clone())
            .map_err(|_| "OpenAIDE could not prepare its SSH sign-in prompts.".to_string())?;
        *broker = Some(started.clone());
        Ok(started)
    }
}

/// Starts or attaches to the App Server on `computer` and forwards it. The
/// startup screen shows each stage when `show_progress` is set.
pub(crate) async fn connect(
    app: &tauri::AppHandle,
    computer: &RemoteComputer,
    destination: &str,
    show_progress: bool,
) -> Result<LocalHttpConnection, String> {
    let ssh_state = app.state::<DesktopSshState>();
    let mode = take_confirmed_update(app, &computer.id)?;
    let broker = ssh_state.askpass(app)?;
    let source = release_source(app, &ssh_state)?;
    let operation_id = format!("desktop-ssh-connect-{}", uuid::Uuid::new_v4());
    let name = computer.name.clone();
    let progress_app = app.clone();
    let launcher = SshLauncher::new(
        source.release.clone(),
        crate::desktop_remote::logger(),
        Box::new(move |stage| {
            if show_progress {
                crate::desktop_connection::emit_bootstrap_progress(
                    &progress_app,
                    "ssh",
                    stage_message(stage, &name),
                );
            }
        }),
    );
    let environment = broker.environment(&computer.name);
    let destination = destination.to_string();
    let started = Instant::now();
    desktop_log!("desktop_ssh_connect_started operation_id={operation_id}");
    let launched = tauri::async_runtime::spawn_blocking(move || {
        let mut transport = SystemSsh::new(&destination, environment);
        let mut fetch = || source.open();
        launcher.launch(&mut transport, mode, &mut fetch, &operation_id)
    })
    .await
    .map_err(|_| "The SSH connection task failed.".to_string())?;
    desktop_log!(
        "desktop_ssh_connect_completed outcome={} duration_ms={}",
        match &launched {
            Ok(Launched::Ready(_)) => "ready",
            Ok(Launched::UpdateRequired { .. }) => "update_required",
            Err(_) => "failure",
        },
        started.elapsed().as_millis()
    );
    let failure = |kind, running_version, stage: Stage, error_class: String| SshFailure {
        computer_id: computer.id.clone(),
        kind,
        running_version,
        stage: stage.as_str(),
        error_class,
    };
    match launched {
        Ok(Launched::Ready(ready)) => {
            set(&ssh_state.failure, None);
            set(
                &ssh_state.status,
                Some(SshStatus {
                    computer_id: computer.id.clone(),
                    running_version: ready.app_version.clone(),
                    update_available: ready.update_available,
                    suggest_linger: ready.suggest_linger,
                }),
            );
            // Replacing the forward ends the previous `ssh`.
            set(&ssh_state.forward, Some(ready.forward));
            Ok(LocalHttpConnection {
                kind: "localHttp".to_string(),
                endpoint_url: ready.endpoint_url,
                auth_token: ready.auth_token,
            })
        }
        Ok(Launched::UpdateRequired { running_version }) => {
            set(
                &ssh_state.failure,
                Some(failure(
                    "updateRequired",
                    Some(running_version.clone()),
                    Stage::Start,
                    "update_required".to_string(),
                )),
            );
            Err(format!(
                "{} runs an older OpenAIDE ({running_version}) that this version cannot use. Update it to continue.",
                computer.name
            ))
        }
        Err(error) => {
            let running_version = match &error {
                LaunchError::ServerNewer { running_version } => Some(running_version.clone()),
                _ => None,
            };
            set(
                &ssh_state.failure,
                Some(failure(
                    "failed",
                    running_version,
                    error.stage(),
                    error.class().to_string(),
                )),
            );
            Err(message(&error, &computer.name))
        }
    }
}

/// Brings the SSH Computer's App Server back after sleep or after its keeper
/// let it go. `None` when no SSH Computer is in use.
pub(crate) async fn ensure(
    app: &tauri::AppHandle,
) -> Option<Result<Option<LocalHttpConnection>, String>> {
    let connections = app.state::<DesktopConnections>();
    let id = connections.computer_in_use()??;
    let computer = app
        .state::<DesktopState>()
        .preferences
        .lock()
        .ok()?
        .remote_computers
        .iter()
        .find(|computer| computer.id == id)
        .cloned()?;
    let destination = computer.ssh.clone()?;
    Some(
        async {
            if let Some((connection, _)) = connections.in_use()
                && app_server_endpoint_reachable(&connection).await
            {
                return Ok(None);
            }
            let started = Instant::now();
            desktop_log!("desktop_ssh_ensure_started reason_code=endpoint_unreachable");
            let connected = connect(app, &computer, &destination, false).await;
            let connection = match connected {
                Ok(connection) => connection,
                Err(message) => {
                    desktop_log!(
                        "desktop_ssh_ensure_completed outcome=failure duration_ms={}",
                        started.elapsed().as_millis()
                    );
                    return Err(message);
                }
            };
            // A switch away while connecting leaves the window where it went.
            let window_follows = connections.replace_ssh(&computer.id, connection.clone());
            if window_follows {
                let _ = app.emit("desktop-app-server-replaced", &connection);
            }
            desktop_log!(
                "desktop_ssh_ensure_completed outcome=success window_follows={window_follows} duration_ms={}",
                started.elapsed().as_millis()
            );
            Ok(window_follows.then_some(connection))
        }
        .await,
    )
}

/// An update the user confirmed applies to the next start with that computer only.
fn take_confirmed_update(app: &tauri::AppHandle, id: &str) -> Result<StartMode, String> {
    let state = app.state::<DesktopState>();
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    if preferences.ssh_replace.as_deref() != Some(id) {
        return Ok(StartMode::Keep);
    }
    preferences.ssh_replace = None;
    write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    desktop_log!("desktop_ssh_update_confirmed_applied");
    Ok(StartMode::Replace)
}

fn release_source(
    app: &tauri::AppHandle,
    ssh_state: &DesktopSshState,
) -> Result<ReleaseSource, String> {
    let version = app.package_info().version.to_string();
    let wsl_binary = app.state::<DesktopState>().wsl_resource_binary.clone();
    crate::desktop_ssh_release::resolve(&version, &wsl_binary, &ssh_state.cache_root).map_err(
        |class| {
            desktop_log!("desktop_ssh_release_unavailable error_class={class}");
            "This build of OpenAIDE cannot install its App Server on SSH computers.".to_string()
        },
    )
}

#[tauri::command]
pub(crate) fn desktop_ssh_hosts(app: tauri::AppHandle) -> Vec<String> {
    app.path()
        .home_dir()
        .map(|home| crate::desktop_ssh_config::host_aliases(&home.join(".ssh")))
        .unwrap_or_default()
}

/// Remembers an SSH Computer by its `ssh` destination, then switches to it like
/// to a paired computer: `switched` (the window reloads), `alreadyInUse`, or
/// `cancelled`. A computer that cannot be reached stays saved and unused.
#[tauri::command]
pub(crate) async fn desktop_ssh_add(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    destination: String,
    name: Option<String>,
    operation_id: Option<String>,
) -> Result<&'static str, String> {
    let destination = destination.trim().to_string();
    if !is_destination(&destination) {
        return Err("Enter an SSH host alias or user@host.".to_string());
    }
    let name = name
        .map(|name| openaide_remote_client::clamp_label(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| openaide_remote_client::clamp_label(&destination));
    let id = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|_| "desktop bootstrap preferences unavailable")?;
        let id = preferences
            .remote_computers
            .iter()
            .find(|computer| computer.ssh.as_deref() == Some(destination.as_str()))
            .map(|computer| computer.id.clone())
            .unwrap_or_else(|| format!("ssh-{}", uuid::Uuid::new_v4().simple()));
        preferences.remember(RemoteComputer {
            id: id.clone(),
            name,
            ssh: Some(destination),
        });
        write_bootstrap_preferences(&state.preferences_path, &preferences)?;
        id
    };
    desktop_log!("desktop_ssh_computer_added");
    crate::desktop_remote::use_computer(&app, Some(id), operation_id).await
}

#[tauri::command]
pub(crate) fn desktop_ssh_status(state: tauri::State<'_, DesktopSshState>) -> Option<SshStatus> {
    state.status.lock().ok()?.clone()
}

#[tauri::command]
pub(crate) fn desktop_ssh_failure(state: tauri::State<'_, DesktopSshState>) -> Option<SshFailure> {
    state.failure.lock().ok()?.clone()
}

/// Replaces the App Server on the SSH Computer in use. The WebView asks only after
/// showing what the replacement interrupts. Connected, it reconnects in place and the
/// window follows the new forward (`desktop-app-server-replaced`), as after a sleep;
/// on the startup page, where nothing is connected, the window reloads to start again,
/// and that start applies the update. Answers `replaced` or `reload`.
#[tauri::command]
pub(crate) async fn desktop_ssh_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    connections: tauri::State<'_, DesktopConnections>,
) -> Result<&'static str, String> {
    let _exclusive = connections.exclusive().await;
    let computer = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|_| "desktop bootstrap preferences unavailable")?;
        let computer = preferences
            .remote()
            .filter(|computer| computer.ssh.is_some())
            .cloned()
            .ok_or_else(|| "No SSH computer is in use.".to_string())?;
        preferences.ssh_replace = Some(computer.id.clone());
        write_bootstrap_preferences(&state.preferences_path, &preferences)?;
        computer
    };
    desktop_log!("desktop_ssh_update_confirmed");
    if connections.computer_in_use() != Some(Some(computer.id.clone())) {
        desktop_log!("desktop_ssh_update_completed outcome=reload");
        return Ok("reload");
    }
    let started = Instant::now();
    let destination = computer.ssh.clone().unwrap_or_default();
    let connection = match connect(&app, &computer, &destination, false).await {
        Ok(connection) => connection,
        Err(message) => {
            desktop_log!(
                "desktop_ssh_update_completed outcome=failure duration_ms={}",
                started.elapsed().as_millis()
            );
            return Err(message);
        }
    };
    let window_follows = connections.replace_ssh(&computer.id, connection.clone());
    if window_follows {
        let _ = app.emit("desktop-app-server-replaced", &connection);
    }
    desktop_log!(
        "desktop_ssh_update_completed outcome=replaced window_follows={window_follows} duration_ms={}",
        started.elapsed().as_millis()
    );
    Ok("replaced")
}

/// Ends the keeper on an SSH Computer. Its App Server then stops once no App
/// Shell uses it.
#[tauri::command]
pub(crate) async fn desktop_ssh_stop(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    ssh_state: tauri::State<'_, DesktopSshState>,
    id: String,
) -> Result<&'static str, String> {
    let computer = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?
        .remote_computers
        .iter()
        .find(|computer| computer.id == id)
        .cloned()
        .ok_or_else(|| "This computer is no longer saved.".to_string())?;
    let destination = computer
        .ssh
        .clone()
        .ok_or_else(|| "This computer is not an SSH computer.".to_string())?;
    let environment = ssh_state.askpass(&app)?.environment(&computer.name);
    let source = release_source(&app, &ssh_state)?;
    let launcher = SshLauncher::new(
        source.release,
        crate::desktop_remote::logger(),
        Box::new(|_| {}),
    );
    let operation_id = format!("desktop-ssh-stop-{}", uuid::Uuid::new_v4());
    let stopped = tauri::async_runtime::spawn_blocking(move || {
        launcher.stop(
            &mut SystemSsh::new(&destination, environment),
            &operation_id,
        )
    })
    .await
    .map_err(|_| "The SSH connection task failed.".to_string())?;
    match stopped {
        Ok(StopOutcome::Stopped) => Ok("stopped"),
        Ok(StopOutcome::NotRunning) => Ok("notRunning"),
        Ok(StopOutcome::StillRunning) => Ok("stillRunning"),
        Err(error) => Err(message(&error, &computer.name)),
    }
}

#[tauri::command]
pub(crate) fn desktop_ssh_prompts(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopSshState>,
) -> Vec<SshPrompt> {
    state
        .askpass(&app)
        .map(|broker| broker.pending())
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) fn desktop_ssh_prompt_answer(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopSshState>,
    id: u64,
    answer: Option<String>,
) -> bool {
    state
        .askpass(&app)
        .map(|broker| broker.answer(id, answer))
        .unwrap_or(false)
}

fn set<T>(slot: &Mutex<Option<T>>, value: Option<T>) {
    if let Ok(mut slot) = slot.lock() {
        *slot = value;
    }
}

fn stage_message(stage: Stage, name: &str) -> String {
    match stage {
        Stage::Probe => format!("Connecting to {name}"),
        Stage::Fetch => "Downloading the App Server".to_string(),
        Stage::Upload => format!("Installing the App Server on {name}"),
        Stage::Start => format!("Starting the App Server on {name}"),
        Stage::Forward => format!("Opening the connection to {name}"),
    }
}

fn message(error: &LaunchError, name: &str) -> String {
    match error {
        LaunchError::UnsupportedPlatform => {
            format!(
                "{name} is not a Linux x64 computer. OpenAIDE runs on SSH computers with Linux x64 only."
            )
        }
        LaunchError::Fetch(_) => {
            "OpenAIDE could not download its App Server. Check this computer's internet connection."
                .to_string()
        }
        LaunchError::Install("checksum_tool_missing") => {
            format!("{name} has no sha256sum, which OpenAIDE needs to verify its install.")
        }
        LaunchError::Install(_) => format!(
            "OpenAIDE could not install its App Server on {name}. Check the free space in your home directory there."
        ),
        LaunchError::ServerNewer { running_version } => format!(
            "{name} runs a newer OpenAIDE ({running_version}). Update OpenAIDE on this computer first."
        ),
        LaunchError::ServerVersionUnknown => format!(
            "{name} runs an OpenAIDE whose version is unknown. Stop it there, then try again."
        ),
        LaunchError::UnreadableAnswer(_) => format!(
            "{name} gave an unexpected answer. Its login shell may print text in non-interactive sessions."
        ),
        LaunchError::Keeper(class) => {
            format!("OpenAIDE could not start on {name} ({class}).")
        }
        LaunchError::Transport { error, .. } => match error.class {
            "ssh_missing" => {
                "OpenAIDE needs the ssh command. Install OpenSSH, then try again.".to_string()
            }
            "auth_failed" => {
                format!("{name} did not accept the sign-in. Check your SSH key or password.")
            }
            "host_key_rejected" => format!(
                "{name}'s host key does not match the one this computer knows. Check known_hosts before connecting."
            ),
            "host_unknown" => format!("OpenAIDE could not find {name}. Check the host name."),
            "ssh_config_invalid" => {
                "Your SSH configuration has an error. Run ssh in a terminal to see it.".to_string()
            }
            "forward_failed" | "forward_timeout" | "local_port_unavailable" => format!(
                "OpenAIDE could not open its connection to {name}. A LocalForward in your SSH configuration may be in the way."
            ),
            _ => format!("{name} is unreachable over SSH."),
        },
    }
}

#[cfg(test)]
#[path = "desktop_ssh_tests.rs"]
mod tests;
