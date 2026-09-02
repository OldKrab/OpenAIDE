#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Read};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

mod desktop_runtime;
mod desktop_support_export;
mod desktop_update;
mod desktop_update_receipt;
mod desktop_update_schedule;
mod desktop_update_security;
mod desktop_update_shutdown;
#[cfg(test)]
mod desktop_update_tests;
#[cfg(target_os = "macos")]
mod macos_webview_resize;
mod wsl_runtime;

use desktop_runtime::{
    DesktopBootstrapPreferences, DesktopRuntimeEnvironment, DesktopRuntimePaths,
    PreparedProjectFolder, prepare_project_path,
};
use desktop_support_export::{
    download_support_export, remember_export_directory, support_export_download_url,
    validate_export_label,
};
use desktop_update::DesktopUpdateState;
use wsl_runtime::{discover_wsl_distros, launch_wsl_app_server_handoff, translate_path_with_wsl};

const HANDOFF_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_HANDOFF_LINE_BYTES: usize = 8 * 1024;
const MIN_AUTH_TOKEN_BYTES: usize = 32;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalHttpConnection {
    kind: String,
    endpoint_url: String,
    auth_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopBootstrap {
    client_instance_id: String,
    connection: LocalHttpConnection,
    platform: &'static str,
    runtime_environment: DesktopRuntimeEnvironment,
    runtime_options: DesktopRuntimeOptions,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopRuntimeOptions {
    wsl_distros: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopBootstrapContext {
    environment: DesktopRuntimeEnvironment,
    can_recover: bool,
}

#[derive(Clone, Debug, Serialize)]
struct DesktopBootstrapProgress {
    message: String,
    stage: &'static str,
}

struct DesktopState {
    client_instance_id: String,
    connection: Mutex<Option<LocalHttpConnection>>,
    runtime_paths: DesktopRuntimePaths,
    preferences: Mutex<DesktopBootstrapPreferences>,
    preferences_path: PathBuf,
    wsl_resource_binary: PathBuf,
}

impl DesktopState {
    fn new(
        runtime_paths: DesktopRuntimePaths,
        preferences: DesktopBootstrapPreferences,
        preferences_path: PathBuf,
        wsl_resource_binary: PathBuf,
    ) -> Self {
        Self {
            client_instance_id: format!("desktop-{}", uuid::Uuid::new_v4()),
            connection: Mutex::new(None),
            runtime_paths,
            preferences: Mutex::new(preferences),
            preferences_path,
            wsl_resource_binary,
        }
    }

    /// The native updater waits for this Desktop-owned listener to disappear before
    /// handing the signed bundle to the platform installer.
    fn app_server_endpoint_url(&self) -> Result<String, String> {
        self.connection
            .lock()
            .map_err(|_| "Desktop runtime state is unavailable.".to_string())?
            .as_ref()
            .map(|connection| connection.endpoint_url.clone())
            .ok_or_else(|| "Desktop runtime is not connected.".to_string())
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // The WebView receives no updater capability. Native commands below
        // expose the fixed, stateful operations owned by Desktop Update. The
        // plugin's required static config deliberately has no usable key;
        // each check builds an updater from compiled Release/Recovery trust.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_local_data = app.path().app_local_data_dir()?;
            // Tauri's `executable_dir` means a user-level bin directory and is
            // unsupported on Windows and macOS; the sidecar lives by this process.
            let executable_path = std::env::current_exe()?;
            let preferences_path = app_local_data.join("desktop-bootstrap.json");
            let preferences = read_bootstrap_preferences(&preferences_path);
            let resource_dir = app.path().resource_dir()?;
            app.manage(DesktopState::new(
                DesktopRuntimePaths::resolve(&app_local_data, &executable_path)?,
                preferences,
                preferences_path,
                resource_dir.join("wsl").join("openaide-app-server"),
            ));
            app.manage(DesktopUpdateState::for_build(
                app_local_data.join("desktop-update-receipt.json"),
                app_local_data.join("desktop-update-schedule.json"),
            ));
            #[cfg(target_os = "macos")]
            install_macos_menu(app)?;
            #[cfg(target_os = "macos")]
            configure_macos_webview_resize(app)?;
            #[cfg(all(target_os = "macos", debug_assertions))]
            watch_development_runner(app);
            Ok(())
        })
        .on_menu_event(|app, event| {
            let _ = app.emit("desktop-command", event.id().as_ref());
        })
        .on_window_event(|window, event| {
            #[cfg(target_os = "windows")]
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                // Native close and the custom caption button must cross the same
                // detach boundary as explicit Quit before this process disappears.
                api.prevent_close();
                let _ = window.emit("desktop-command", "quit");
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop_bootstrap,
            desktop_bootstrap_context,
            complete_desktop_quit,
            record_desktop_telemetry,
            prepare_desktop_native_zoom,
            perform_desktop_native_zoom,
            set_desktop_runtime_environment,
            desktop_prepare_project_folder,
            desktop_dismiss_path_warning,
            desktop_save_support_export,
            recover_desktop_runtime_environment,
            desktop_update::desktop_update_snapshot,
            desktop_update::desktop_check_for_update,
            desktop_update::desktop_auto_check_for_update,
            desktop_update::desktop_download_update,
            desktop_update::desktop_cancel_update_download,
            desktop_update::desktop_install_update,
            desktop_update::desktop_update_mark_interactive
        ])
        .run(tauri::generate_context!())
        .expect("failed to run OpenAIDE Desktop App");
}

#[tauri::command]
async fn desktop_save_support_export(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DesktopState>,
    file_handle_id: String,
    label: String,
) -> Result<bool, String> {
    validate_export_label(&label)?;
    let export_directory = state
        .preferences
        .lock()
        .map_err(|_| "Desktop preferences are unavailable.".to_string())?
        .support_export_directory
        .clone();
    let connection = state
        .connection
        .lock()
        .map_err(|_| "Desktop runtime state is unavailable.".to_string())?
        .clone()
        .ok_or_else(|| "Desktop runtime is not connected.".to_string())?;
    let url = support_export_download_url(
        &connection.endpoint_url,
        &state.client_instance_id,
        &file_handle_id,
    )?;
    let operation_id = format!("desktop-support-export-{}", uuid::Uuid::new_v4());
    let started_at = Instant::now();
    eprintln!("desktop_support_export_save_started operation_id={operation_id} attempt_count=1");

    let mut dialog = app
        .dialog()
        .file()
        .set_title("Export OpenAIDE Support Diagnostics")
        .set_file_name(&label)
        .set_parent(&window)
        .add_filter("ZIP archive", &["zip"]);
    if let Some(directory) = export_directory {
        dialog = dialog.set_directory(directory);
    }
    let (destination_sender, destination_receiver) = tokio::sync::oneshot::channel();
    dialog.save_file(move |destination| {
        let _ = destination_sender.send(destination);
    });
    let destination = destination_receiver
        .await
        .map_err(|_| "The support export Save dialog closed unexpectedly.".to_string())?;
    let Some(destination) = destination else {
        eprintln!(
            "desktop_support_export_save_completed operation_id={operation_id} outcome=cancelled duration_ms={} attempt_count=1",
            started_at.elapsed().as_millis()
        );
        return Ok(false);
    };
    let destination = match destination.into_path() {
        Ok(destination) => destination,
        Err(_) => {
            eprintln!(
                "desktop_support_export_save_completed operation_id={operation_id} outcome=failure error_kind=invalid_destination duration_ms={} attempt_count=1",
                started_at.elapsed().as_millis()
            );
            return Err("The selected support export path is invalid.".to_string());
        }
    };
    let bytes = match download_support_export(url, &connection.auth_token).await {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "desktop_support_export_save_completed operation_id={operation_id} outcome=failure error_kind=download duration_ms={} attempt_count=1",
                started_at.elapsed().as_millis()
            );
            return Err(error);
        }
    };
    if std::fs::write(&destination, bytes).is_err() {
        eprintln!(
            "desktop_support_export_save_completed operation_id={operation_id} outcome=failure error_kind=file_write duration_ms={} attempt_count=1",
            started_at.elapsed().as_millis()
        );
        return Err("Unable to save the support export to the selected file.".to_string());
    }

    match state.preferences.lock() {
        Ok(mut preferences) => {
            remember_export_directory(&mut preferences, &destination);
            if write_bootstrap_preferences(&state.preferences_path, &preferences).is_err() {
                eprintln!(
                    "desktop_support_export_directory_remember_failed operation_id={operation_id} error_kind=preference_write"
                );
            }
        }
        Err(_) => eprintln!(
            "desktop_support_export_directory_remember_failed operation_id={operation_id} error_kind=preference_lock"
        ),
    }
    eprintln!(
        "desktop_support_export_save_completed operation_id={operation_id} outcome=success duration_ms={} attempt_count=1",
        started_at.elapsed().as_millis()
    );
    Ok(true)
}

#[tauri::command]
fn desktop_bootstrap_context(
    state: tauri::State<'_, DesktopState>,
) -> Result<DesktopBootstrapContext, String> {
    let preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    Ok(DesktopBootstrapContext {
        environment: preferences.environment.clone(),
        can_recover: preferences.previous_environment.is_some(),
    })
}

#[tauri::command]
fn complete_desktop_quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg(target_os = "macos")]
fn configure_macos_webview_resize(app: &mut tauri::App) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .expect("main desktop window must exist during setup");
    macos_webview_resize::configure(&window)
}

/// Stops an F5 app that outlives Tauri's development runner during a rebuild.
/// Release bundles never receive the parent marker and never start this watch.
#[cfg(all(target_os = "macos", debug_assertions))]
fn watch_development_runner(app: &tauri::App) {
    let Some(parent_pid) = std::env::var("OPENAIDE_DEVELOPMENT_RUNNER_PID")
        .ok()
        .and_then(|value| value.parse::<libc::pid_t>().ok())
    else {
        return;
    };
    let app_handle = app.handle().clone();
    eprintln!("desktop_development_runner_watch_started");
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(400));
            // A permission error still means the process exists; every other error
            // means Tauri's watcher replaced the owning runner.
            let alive = unsafe { libc::kill(parent_pid, 0) } == 0
                || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
            if alive {
                continue;
            }
            eprintln!("desktop_development_runner_watch_completed outcome=parent_ended");
            app_handle.exit(0);
            break;
        }
    });
}

#[cfg(target_os = "macos")]
fn install_macos_menu(app: &mut tauri::App) -> tauri::Result<()> {
    let new_task = MenuItemBuilder::with_id("new-task", "New Task")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let open_project = MenuItemBuilder::with_id("open-project", "Add Project Folder…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let check_for_updates =
        MenuItemBuilder::with_id("check-for-updates", "Check for Updates…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit OpenAIDE")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;
    let application = SubmenuBuilder::new(app, "OpenAIDE")
        .items(&[
            &settings,
            &check_for_updates,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ])
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .items(&[&new_task, &open_project])
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .items(&[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ])
        .build()?;
    app.set_menu(
        MenuBuilder::new(app)
            .items(&[&application, &file, &edit])
            .build()?,
    )?;
    Ok(())
}

#[tauri::command]
async fn desktop_bootstrap(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<DesktopBootstrap, String> {
    let environment = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?
        .environment
        .clone();
    let runtime_options = DesktopRuntimeOptions {
        wsl_distros: discover_wsl_distros(),
    };
    if let Some(connection) = state
        .connection
        .lock()
        .map_err(|_| "desktop connection state unavailable")?
        .clone()
    {
        return Ok(DesktopBootstrap {
            client_instance_id: state.client_instance_id.clone(),
            connection,
            platform: std::env::consts::OS,
            runtime_environment: environment,
            runtime_options,
        });
    }

    let operation_id = format!("desktop-bootstrap-{}", uuid::Uuid::new_v4());
    let started_at = Instant::now();
    eprintln!("desktop_app_server_handoff_started operation_id={operation_id}");
    let runtime_paths = state.runtime_paths.clone();
    let wsl_resource_binary = state.wsl_resource_binary.clone();
    let launch_environment = environment.clone();
    let progress_app = app.clone();
    let connection = tauri::async_runtime::spawn_blocking(move || {
        launch_app_server_handoff(
            &runtime_paths,
            &launch_environment,
            &wsl_resource_binary,
            |stage, message| emit_bootstrap_progress(&progress_app, stage, message),
        )
    })
    .await
    .map_err(|_| {
        eprintln!(
            "desktop_app_server_handoff_completed operation_id={operation_id} outcome=failure stage=task_join duration_ms={}",
            started_at.elapsed().as_millis()
        );
        "App Server handoff task failed".to_string()
    })?
    .map_err(|error| {
        eprintln!(
            "desktop_app_server_handoff_completed operation_id={operation_id} outcome=failure stage={} duration_ms={}",
            error.stage,
            started_at.elapsed().as_millis()
        );
        error.user_message
    })?;
    eprintln!(
        "desktop_app_server_handoff_completed operation_id={operation_id} outcome=success duration_ms={}",
        started_at.elapsed().as_millis()
    );
    *state
        .connection
        .lock()
        .map_err(|_| "desktop connection state unavailable")? = Some(connection.clone());
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    if preferences.previous_environment.take().is_some() {
        write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    }
    drop(preferences);
    Ok(DesktopBootstrap {
        client_instance_id: state.client_instance_id.clone(),
        connection,
        platform: std::env::consts::OS,
        runtime_environment: environment,
        runtime_options,
    })
}

struct HandoffFailure {
    stage: &'static str,
    user_message: String,
}

fn emit_bootstrap_progress(app: &tauri::AppHandle, stage: &'static str, message: String) {
    let _ = app.emit(
        "desktop-bootstrap-progress",
        DesktopBootstrapProgress { message, stage },
    );
}

fn read_bootstrap_preferences(path: &Path) -> DesktopBootstrapPreferences {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_bootstrap_preferences(
    path: &Path,
    preferences: &DesktopBootstrapPreferences,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "OpenAIDE could not save its Desktop environment.".to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(preferences)
        .map_err(|_| "OpenAIDE could not save its Desktop environment.".to_string())?;
    std::fs::write(path, bytes)
        .map_err(|_| "OpenAIDE could not save its Desktop environment.".to_string())
}

#[tauri::command]
fn set_desktop_runtime_environment(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    environment: DesktopRuntimeEnvironment,
) -> Result<(), String> {
    if let DesktopRuntimeEnvironment::Wsl { distro } = &environment {
        let distros = discover_wsl_distros();
        if !distros
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(distro))
        {
            return Err("The selected WSL distribution is unavailable.".to_string());
        }
    }
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    if preferences.environment == environment {
        return Ok(());
    }
    preferences.previous_environment = Some(preferences.environment.clone());
    preferences.environment = environment;
    write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    drop(preferences);
    app.restart();
}

#[tauri::command]
fn recover_desktop_runtime_environment(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<(), String> {
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    let previous = preferences
        .previous_environment
        .take()
        .ok_or_else(|| "No previous Desktop environment is available.".to_string())?;
    preferences.environment = previous;
    write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    drop(preferences);
    app.restart();
}

#[tauri::command]
fn desktop_dismiss_path_warning(
    state: tauri::State<'_, DesktopState>,
    key: String,
) -> Result<(), String> {
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?;
    if !preferences.dismissed_path_warnings.contains(&key) {
        preferences.dismissed_path_warnings.push(key);
        write_bootstrap_preferences(&state.preferences_path, &preferences)?;
    }
    Ok(())
}

#[tauri::command]
fn desktop_prepare_project_folder(
    state: tauri::State<'_, DesktopState>,
    path: String,
) -> Result<PreparedProjectFolder, String> {
    let preferences = state
        .preferences
        .lock()
        .map_err(|_| "desktop bootstrap preferences unavailable")?
        .clone();
    let environment = preferences.environment.clone();
    prepare_project_path(
        &path,
        &environment,
        &preferences.dismissed_path_warnings,
        |windows_path| match &environment {
            DesktopRuntimeEnvironment::Wsl { distro } => {
                translate_path_with_wsl(distro, windows_path)
            }
            DesktopRuntimeEnvironment::Native => Ok(windows_path.to_string()),
        },
    )
    .map_err(|error| error.to_string())
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
    let stdout = child.stdout.take().ok_or_else(|| {
        failure(
            "stdout_missing",
            "The App Server did not provide startup information.",
        )
    })?;
    let line = read_handoff_line(stdout, &mut child)?;
    let connection: LocalHttpConnection = serde_json::from_str(line.trim()).map_err(|_| {
        failure(
            "json_invalid",
            "The App Server returned invalid startup information.",
        )
    })?;
    validate_connection(&connection)?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(connection)
}

fn read_handoff_line(
    stdout: impl Read + Send + 'static,
    child: &mut Child,
) -> Result<String, HandoffFailure> {
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
        Ok(Ok(_)) => {
            let _ = child.kill();
            Err(failure(
                "line_invalid",
                "The App Server returned invalid startup information.",
            ))
        }
        Ok(Err(_)) => {
            let _ = child.kill();
            Err(failure(
                "line_read",
                "OpenAIDE could not read App Server startup information.",
            ))
        }
        Err(_) => {
            let _ = child.kill();
            Err(failure(
                "timeout",
                "The App Server did not become ready in time.",
            ))
        }
    }
}

fn create_directory(path: &std::path::Path, stage: &'static str) -> Result<(), HandoffFailure> {
    std::fs::create_dir_all(path)
        .map_err(|_| failure(stage, "OpenAIDE could not prepare its local storage."))
}

fn validate_connection(connection: &LocalHttpConnection) -> Result<(), HandoffFailure> {
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

fn failure(stage: &'static str, user_message: &str) -> HandoffFailure {
    HandoffFailure {
        stage,
        user_message: user_message.to_string(),
    }
}

#[tauri::command]
fn record_desktop_telemetry(payload: serde_json::Value) {
    let event = payload
        .get("event")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let surface = payload
        .get("surface")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    eprintln!("desktop_webview_event event={event} surface={surface}");
}

#[tauri::command]
fn prepare_desktop_native_zoom(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return macos_webview_resize::prepare_native_zoom(&window);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Ok(())
    }
}

#[tauri::command]
fn perform_desktop_native_zoom(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return macos_webview_resize::perform_native_zoom(&window);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Ok(())
    }
}
