//! Starts this computer's App Server for the Desktop and reads where it listens.
//!
//! The App Server prints one handoff line with its loopback endpoint and token.
//! A WSL environment runs the same handoff inside the distribution.

use std::io::{BufRead, BufReader, Read};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[cfg(target_os = "windows")]
use crate::CREATE_NO_WINDOW;
use crate::desktop_connection::emit_bootstrap_progress;
#[cfg(target_os = "macos")]
use crate::macos_shell_path;
use crate::desktop_runtime::{DesktopRuntimeEnvironment, DesktopRuntimePaths};
use crate::startup_process::StartupChild;
use crate::wsl_runtime::launch_wsl_app_server_handoff;

const HANDOFF_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_HANDOFF_LINE_BYTES: usize = 8 * 1024;
const MIN_AUTH_TOKEN_BYTES: usize = 32;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalHttpConnection {
    pub(crate) kind: String,
    pub(crate) endpoint_url: String,
    pub(crate) auth_token: String,
}

pub(crate) struct HandoffFailure {
    pub(crate) stage: &'static str,
    pub(crate) user_message: String,
}

/// Runs the App Server handoff and always reports one terminal line. Callers own the
/// decision to ask for a process; the handoff itself may elect an App Server that is
/// already running. Progress goes to the startup window only while it is listening.
pub(crate) async fn launch_app_server_for_desktop(
    runtime_paths: DesktopRuntimePaths,
    wsl_resource_binary: PathBuf,
    environment: DesktopRuntimeEnvironment,
    progress_app: Option<tauri::AppHandle>,
    operation_id: String,
) -> Result<LocalHttpConnection, String> {
    let started_at = Instant::now();
    desktop_log!("desktop_app_server_handoff_started operation_id={operation_id}");
    let joined = tauri::async_runtime::spawn_blocking(move || {
        launch_app_server_handoff(
            &runtime_paths,
            &environment,
            &wsl_resource_binary,
            |stage, message| {
                if let Some(app) = progress_app.as_ref() {
                    emit_bootstrap_progress(app, stage, message);
                }
            },
        )
    })
    .await;
    match joined {
        Err(_) => {
            desktop_log!(
                "desktop_app_server_handoff_completed operation_id={operation_id} outcome=failure stage=task_join duration_ms={}",
                started_at.elapsed().as_millis()
            );
            Err("App Server handoff task failed".to_string())
        }
        Ok(Err(error)) => {
            desktop_log!(
                "desktop_app_server_handoff_completed operation_id={operation_id} outcome=failure stage={} duration_ms={}",
                error.stage,
                started_at.elapsed().as_millis()
            );
            Err(error.user_message)
        }
        Ok(Ok(connection)) => {
            desktop_log!(
                "desktop_app_server_handoff_completed operation_id={operation_id} outcome=success duration_ms={}",
                started_at.elapsed().as_millis()
            );
            Ok(connection)
        }
    }
}

/// A live App Server answers HTTP on its published loopback endpoint. Any HTTP status
/// proves a listener; the endpoint is loopback-only and token-guarded, so a 401 still
/// means the process is there. Only a connection failure means the process is gone.
pub(crate) async fn app_server_endpoint_reachable(connection: &LocalHttpConnection) -> bool {
    let Ok(url) = url::Url::parse(&connection.endpoint_url) else {
        return false;
    };
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(750))
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
    else {
        return false;
    };
    client
        .get(url)
        .bearer_auth(&connection.auth_token)
        .send()
        .await
        .is_ok()
}

fn launch_app_server_handoff(
    runtime_paths: &DesktopRuntimePaths,
    environment: &DesktopRuntimeEnvironment,
    wsl_resource_binary: &Path,
    progress: impl Fn(&'static str, String),
) -> Result<LocalHttpConnection, HandoffFailure> {
    if let DesktopRuntimeEnvironment::Wsl { distro } = environment {
        return launch_wsl_app_server_handoff(distro, wsl_resource_binary, progress);
    }
    create_directory(&runtime_paths.storage_root, "storage_create")?;
    create_directory(&runtime_paths.runtime_root, "runtime_create")?;

    let mut command = Command::new(&runtime_paths.app_server_binary);
    #[cfg(target_os = "macos")]
    macos_shell_path::apply_to(&mut command).map_err(|_| {
        failure(
            "shell_environment",
            "OpenAIDE could not load your terminal PATH. Check your shell startup files and retry.",
        )
    })?;
    command
        .env("OPENAIDE_APP_SERVER_PROTOCOL", "app-server-handoff")
        .env("OPENAIDE_STORAGE_ROOT", &runtime_paths.storage_root)
        .env("OPENAIDE_RUNTIME_ROOT", &runtime_paths.runtime_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(target_os = "windows")]
    // The App Server remains a console binary for VSIX use, but Desktop owns it
    // as a background child and must not flash a second console window.
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|_| failure("process_spawn", "OpenAIDE could not start its App Server."))?;
    let mut startup = StartupChild::new(&mut child);
    let stdout = startup.take_stdout().ok_or_else(|| {
        failure(
            "stdout_missing",
            "The App Server did not provide startup information.",
        )
    })?;
    let line = read_handoff_line(stdout)?;
    let connection: LocalHttpConnection = serde_json::from_str(line.trim()).map_err(|_| {
        failure(
            "json_invalid",
            "The App Server returned invalid startup information.",
        )
    })?;
    validate_connection(&connection)?;
    startup.accept();
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(connection)
}

pub(crate) fn read_handoff_line(stdout: impl Read + Send + 'static) -> Result<String, HandoffFailure> {
    let (sender, receiver) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout)
            .take((MAX_HANDOFF_LINE_BYTES + 1) as u64)
            .read_line(&mut line)
            .map(|_| line);
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(HANDOFF_TIMEOUT) {
        Ok(Ok(line)) if !line.trim().is_empty() && line.len() <= MAX_HANDOFF_LINE_BYTES => Ok(line),
        Ok(Ok(_)) => Err(failure(
            "line_invalid",
            "The App Server returned invalid startup information.",
        )),
        Ok(Err(_)) => Err(failure(
            "line_read",
            "OpenAIDE could not read App Server startup information.",
        )),
        Err(_) => Err(failure(
            "timeout",
            "The App Server did not become ready in time.",
        )),
    }
}

fn create_directory(path: &std::path::Path, stage: &'static str) -> Result<(), HandoffFailure> {
    std::fs::create_dir_all(path)
        .map_err(|_| failure(stage, "OpenAIDE could not prepare its local storage."))
}

pub(crate) fn validate_connection(connection: &LocalHttpConnection) -> Result<(), HandoffFailure> {
    if connection.kind != "localHttp" || connection.auth_token.len() < MIN_AUTH_TOKEN_BYTES {
        return Err(failure(
            "connection_invalid",
            "The App Server returned invalid startup information.",
        ));
    }
    let endpoint = url::Url::parse(&connection.endpoint_url).map_err(|_| {
        failure(
            "endpoint_invalid",
            "The App Server returned an invalid local endpoint.",
        )
    })?;
    let loopback = endpoint.scheme() == "http"
        && matches!(endpoint.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if !loopback {
        return Err(failure(
            "endpoint_not_loopback",
            "The App Server returned an unsafe endpoint.",
        ));
    }
    Ok(())
}

pub(crate) fn failure(stage: &'static str, user_message: &str) -> HandoffFailure {
    HandoffFailure {
        stage,
        user_message: user_message.to_string(),
    }
}

#[cfg(test)]
#[path = "desktop_server_recovery_tests.rs"]
mod tests;
