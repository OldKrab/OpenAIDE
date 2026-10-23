//! Which App Server the Desktop window uses (ADR-0070).
//!
//! This computer's App Server starts with the Desktop and stays held until the
//! Desktop quits, whichever computer is in use, so switching away never stops
//! its work or its own Remote Devices. The window uses one App Server at a time:
//! this computer's, a paired computer's through a loopback gateway, or an SSH
//! Computer's through an `ssh` forward (ADR-0069).
//!
//! A switch connects the target while the current one stays in use and replaces
//! it only once connected. The window then reloads into the new one under a new
//! client instance id, so an App Server still holding the previous id in its
//! reconnect grace never hands back stale state.

use std::future::Future;
use std::sync::Mutex;
use std::time::Instant;

use openaide_remote_client::Gateway;
use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::DesktopState;
use crate::desktop_app_server_launch::{
    LocalHttpConnection, app_server_endpoint_reachable, launch_app_server_for_desktop,
};
use crate::desktop_runtime::{DesktopRuntimeEnvironment, write_bootstrap_preferences};
use crate::wsl_runtime::discover_wsl_distros;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopBootstrap {
    client_instance_id: String,
    connection: LocalHttpConnection,
    platform: &'static str,
    runtime_environment: DesktopRuntimeEnvironment,
    runtime_options: DesktopRuntimeOptions,
    /// Names the paired computer whose App Server the window uses instead of this one's.
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_computer: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopRuntimeOptions {
    wsl_distros: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct DesktopBootstrapProgress {
    message: String,
    stage: &'static str,
}

/// Why a computer could not be reached: a stable class for diagnostics and a
/// sentence for the user.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ConnectError {
    pub(crate) class: &'static str,
    pub(crate) message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SwitchOutcome {
    Switched,
    AlreadyInUse,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SwitchError {
    Failed(String),
    Cancelled,
}

/// The App Server the window uses.
struct InUse {
    connection: LocalHttpConnection,
    client_instance_id: String,
    /// The paired computer's id; `None` while this computer is in use.
    computer: Option<String>,
    /// Carries the window's requests to a paired computer; stopped when the
    /// window leaves it.
    gateway: Option<Gateway>,
}

/// Connection ownership for the Desktop process.
#[derive(Default)]
pub(crate) struct DesktopConnections {
    local: Mutex<Option<LocalHttpConnection>>,
    in_use: Mutex<Option<InUse>>,
    /// One bootstrap or switch at a time, held across the connection attempt.
    switching: tokio::sync::Mutex<()>,
    /// One start of this computer's App Server at a time.
    launching: tokio::sync::Mutex<()>,
    /// Ends the switch attempt in progress.
    cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl DesktopConnections {
    /// This computer's App Server, for the native hold and Desktop Update.
    pub(crate) fn local(&self) -> Option<LocalHttpConnection> {
        self.local.lock().ok()?.clone()
    }

    /// The endpoint and client instance id the window uses now.
    pub(crate) fn in_use(&self) -> Option<(LocalHttpConnection, String)> {
        let in_use = self.in_use.lock().ok()?;
        in_use
            .as_ref()
            .map(|in_use| (in_use.connection.clone(), in_use.client_instance_id.clone()))
    }

    /// The paired computer the window uses, `Some(None)` for this computer, and
    /// `None` before the first bootstrap.
    pub(crate) fn computer_in_use(&self) -> Option<Option<String>> {
        let in_use = self.in_use.lock().ok()?;
        in_use.as_ref().map(|in_use| in_use.computer.clone())
    }

    /// Ends the switch attempt in progress; false when there is none.
    pub(crate) fn cancel(&self) -> bool {
        let Some(cancel) = self.cancel.lock().ok().and_then(|mut slot| slot.take()) else {
            return false;
        };
        cancel.send(()).is_ok()
    }

    fn start_using(
        &self,
        computer: Option<String>,
        connection: LocalHttpConnection,
        gateway: Option<Gateway>,
    ) -> Option<Gateway> {
        if computer.is_none() {
            if let Ok(mut local) = self.local.lock() {
                *local = Some(connection.clone());
            }
        }
        let previous = self.in_use.lock().ok()?.replace(InUse {
            connection,
            client_instance_id: format!("desktop-{}", uuid::Uuid::new_v4()),
            computer,
            gateway,
        });
        previous.and_then(|previous| previous.gateway)
    }

    /// Records a new forward to the SSH Computer `id`; true when the window uses
    /// it and must follow the replacement. The SSH forward carries no gateway.
    pub(crate) fn replace_ssh(&self, id: &str, connection: LocalHttpConnection) -> bool {
        let Ok(mut in_use) = self.in_use.lock() else {
            return false;
        };
        match in_use.as_mut() {
            Some(in_use) if in_use.computer.as_deref() == Some(id) => {
                in_use.connection = connection;
                true
            }
            _ => false,
        }
    }

    /// Records a new start of this computer's App Server; true when the window
    /// uses it and must follow the replacement.
    fn replace_local(&self, connection: LocalHttpConnection) -> bool {
        if let Ok(mut local) = self.local.lock() {
            *local = Some(connection.clone());
        }
        let Ok(mut in_use) = self.in_use.lock() else {
            return false;
        };
        match in_use.as_mut() {
            Some(in_use) if in_use.computer.is_none() => {
                in_use.connection = connection;
                true
            }
            _ => false,
        }
    }

    /// Moves the window to `target`, a paired computer's id or this computer for
    /// `None`. `connect` reaches the target while the current App Server stays in
    /// use; `persist` saves the choice for the next start. Nothing changes unless
    /// both succeed, so a failure or a cancel leaves the window where it was.
    pub(crate) async fn switch<C, F>(
        &self,
        target: Option<String>,
        operation_id: &str,
        connect: C,
        persist: impl FnOnce(&Option<String>) -> Result<(), String>,
    ) -> Result<SwitchOutcome, SwitchError>
    where
        C: FnOnce() -> F,
        F: Future<Output = Result<(LocalHttpConnection, Option<Gateway>), ConnectError>>,
    {
        let started_at = Instant::now();
        let _switching = self.switching.lock().await;
        let source = self.computer_in_use();
        if source.as_ref() == Some(&target) {
            return Ok(SwitchOutcome::AlreadyInUse);
        }
        let source_kind = match &source {
            None => "none",
            Some(None) => "this_computer",
            Some(Some(_)) => "paired",
        };
        let target_kind = if target.is_some() {
            "paired"
        } else {
            "this_computer"
        };
        desktop_log!(
            "desktop_computer_switch_started operation_id={operation_id} source_kind={source_kind} target_kind={target_kind}"
        );
        let finish = |outcome: &str, error_class: Option<&str>| {
            desktop_log!(
                "desktop_computer_switch_completed operation_id={operation_id} source_kind={source_kind} target_kind={target_kind} outcome={outcome}{} duration_ms={}",
                error_class
                    .map(|class| format!(" error_class={class}"))
                    .unwrap_or_default(),
                started_at.elapsed().as_millis()
            );
        };

        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        if let Ok(mut slot) = self.cancel.lock() {
            *slot = Some(cancel);
        }
        let connected = tokio::select! {
            connected = connect() => Some(connected),
            _ = cancelled => None,
        };
        if let Ok(mut slot) = self.cancel.lock() {
            slot.take();
        }
        let (connection, gateway) = match connected {
            None => {
                finish("cancelled", None);
                return Err(SwitchError::Cancelled);
            }
            Some(Err(error)) => {
                finish("failed", Some(error.class));
                return Err(SwitchError::Failed(error.message));
            }
            Some(Ok(connected)) => connected,
        };
        if let Err(message) = persist(&target) {
            if let Some(gateway) = gateway {
                gateway.stop().await;
            }
            finish("failed", Some("preferences_write"));
            return Err(SwitchError::Failed(message));
        }
        if let Some(previous) = self.start_using(target, connection, gateway) {
            previous.stop().await;
        }
        finish("switched", None);
        Ok(SwitchOutcome::Switched)
    }
}

pub(crate) fn emit_bootstrap_progress(
    app: &tauri::AppHandle,
    stage: &'static str,
    message: String,
) {
    let _ = app.emit(
        "desktop-bootstrap-progress",
        DesktopBootstrapProgress { message, stage },
    );
}

/// Reloads the window into the App Server now in use, for a switch the window
/// did not ask for itself.
pub(crate) fn reload_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.reload().is_err() {
            desktop_log!("desktop_window_reload_failed error_kind=webview");
        }
    }
}

#[tauri::command]
pub(crate) async fn desktop_bootstrap(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    connections: tauri::State<'_, DesktopConnections>,
) -> Result<DesktopBootstrap, String> {
    let (environment, remote) = {
        let preferences = state
            .preferences
            .lock()
            .map_err(|_| "desktop bootstrap preferences unavailable")?;
        (
            preferences.environment.clone(),
            preferences.remote().cloned(),
        )
    };
    let runtime_options = DesktopRuntimeOptions {
        wsl_distros: discover_wsl_distros(),
    };
    let _switching = connections.switching.lock().await;
    // A reload, including the one that ends a switch, finds the App Server in use.
    if let Some((connection, client_instance_id)) = connections.in_use() {
        return Ok(DesktopBootstrap {
            client_instance_id,
            connection,
            platform: std::env::consts::OS,
            runtime_environment: environment,
            runtime_options,
            remote_computer: remote.map(|computer| computer.name),
        });
    }

    let operation_id = format!("desktop-bootstrap-{}", uuid::Uuid::new_v4());
    match &remote {
        Some(computer) => {
            // This computer's App Server starts as well, so its own work, its
            // Remote Devices, and switching back never wait for a first visit.
            let background = app.clone();
            tauri::async_runtime::spawn(async move {
                let operation_id = format!("desktop-local-start-{}", uuid::Uuid::new_v4());
                let _ = ensure_local(&background, None, operation_id).await;
            });
            let (connection, gateway) = match &computer.ssh {
                Some(destination) => (
                    crate::desktop_ssh::connect(&app, computer, destination, true).await?,
                    None,
                ),
                None => {
                    emit_bootstrap_progress(
                        &app,
                        "remote",
                        format!("Connecting to {}", computer.name),
                    );
                    let (connection, gateway) = crate::desktop_remote::connect(&app, computer)
                        .await
                        .map_err(|error| error.message)?;
                    (connection, Some(gateway))
                }
            };
            connections.start_using(Some(computer.id.clone()), connection, gateway);
        }
        None => {
            let connection = ensure_local(&app, Some(app.clone()), operation_id).await?;
            connections.start_using(None, connection, None);
        }
    }
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    if preferences.previous_environment.take().is_some() {
        write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    }
    drop(preferences);
    let (connection, client_instance_id) = connections
        .in_use()
        .ok_or_else(|| "desktop connection state unavailable".to_string())?;
    Ok(DesktopBootstrap {
        client_instance_id,
        connection,
        platform: std::env::consts::OS,
        runtime_environment: environment,
        runtime_options,
        remote_computer: remote.map(|computer| computer.name),
    })
}

/// This computer's App Server: the running one while it still answers,
/// otherwise a new start. Progress reaches the startup window only when given.
pub(crate) async fn ensure_local(
    app: &tauri::AppHandle,
    progress: Option<tauri::AppHandle>,
    operation_id: String,
) -> Result<LocalHttpConnection, String> {
    let connections = app.state::<DesktopConnections>();
    let _launching = connections.launching.lock().await;
    let cached = connections.local();
    if let Some(connection) = &cached {
        if app_server_endpoint_reachable(connection).await {
            return Ok(connection.clone());
        }
    }
    let started_at = Instant::now();
    desktop_log!(
        "desktop_app_server_ensure_started operation_id={operation_id} had_connection={} reason_code={}",
        cached.is_some(),
        if cached.is_some() {
            "endpoint_unreachable"
        } else {
            "no_connection"
        }
    );
    let state = app.state::<DesktopState>();
    let environment = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?
        .environment
        .clone();
    let launched = launch_app_server_for_desktop(
        state.runtime_paths.clone(),
        state.wsl_resource_binary.clone(),
        environment,
        progress,
        operation_id.clone(),
    )
    .await;
    let connection = match launched {
        Ok(connection) => connection,
        Err(message) => {
            desktop_log!(
                "desktop_app_server_ensure_completed operation_id={operation_id} outcome=failure duration_ms={}",
                started_at.elapsed().as_millis()
            );
            return Err(message);
        }
    };
    let window_follows = connections.replace_local(connection.clone());
    if window_follows {
        let _ = app.emit("desktop-app-server-replaced", &connection);
    }
    desktop_log!(
        "desktop_app_server_ensure_completed operation_id={operation_id} outcome=success window_follows={window_follows} duration_ms={}",
        started_at.elapsed().as_millis()
    );
    Ok(connection)
}

/// Restores this computer's App Server after machine sleep or an idle shutdown,
/// whichever computer the window uses. Returns the new endpoint when it started
/// again; the window hears of it through the replacement event while it uses it.
#[tauri::command]
pub(crate) async fn desktop_ensure_app_server(
    app: tauri::AppHandle,
) -> Result<Option<LocalHttpConnection>, String> {
    // An SSH Computer's forward and App Server are this Desktop's to restore, as
    // its own are; a paired computer's gateway reopens through its own retries.
    let ssh = crate::desktop_ssh::ensure(&app).await;
    let before = app.state::<DesktopConnections>().local();
    let operation_id = format!("desktop-ensure-app-server-{}", uuid::Uuid::new_v4());
    let connection = ensure_local(&app, None, operation_id).await?;
    if let Some(ssh) = ssh {
        return ssh;
    }
    Ok((before.as_ref() != Some(&connection)).then_some(connection))
}

#[cfg(test)]
#[path = "desktop_connection_tests.rs"]
mod tests;
