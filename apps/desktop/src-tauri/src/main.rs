#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
#[cfg(all(target_os = "macos", debug_assertions))]
use std::thread;
#[cfg(all(target_os = "macos", debug_assertions))]
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, Manager};

// First, so every module below can use `desktop_log!`.
#[macro_use]
mod desktop_log;
mod desktop_app_server_hold;
mod desktop_app_server_launch;
mod desktop_connection;
mod desktop_local_attention;
mod desktop_quit;
mod desktop_remote;
mod desktop_renderer_protocol;
mod desktop_runtime;
mod desktop_secrets;
mod desktop_ssh;
mod desktop_ssh_askpass;
mod desktop_ssh_config;
mod desktop_ssh_release;
mod desktop_ssh_transport;
mod desktop_support_export;
mod desktop_update;
mod desktop_update_receipt;
mod desktop_update_security;
mod desktop_update_shutdown;
#[cfg(test)]
mod desktop_update_tests;
mod desktop_webview_log;
#[cfg(target_os = "macos")]
mod macos_menu;
#[cfg(target_os = "macos")]
mod macos_shell_path;
#[cfg(target_os = "macos")]
mod macos_webview_resize;
mod startup_process;
mod wsl_runtime;

use desktop_runtime::{
    DesktopBootstrapPreferences, DesktopRuntimeEnvironment, DesktopRuntimePaths,
    PreparedProjectFolder, prepare_project_path, read_bootstrap_preferences,
    write_bootstrap_preferences,
};
use desktop_update::DesktopUpdateState;
use wsl_runtime::{discover_wsl_distros, translate_path_with_wsl};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopBootstrapContext {
    environment: DesktopRuntimeEnvironment,
    can_recover: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_computer: Option<String>,
}

/// This Desktop's own settings and where its App Server lives. Which App Server
/// the window uses belongs to [`desktop_connection::DesktopConnections`].
struct DesktopState {
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
            runtime_paths,
            preferences: Mutex::new(preferences),
            preferences_path,
            wsl_resource_binary,
        }
    }
}

fn main() {
    // `ssh` starts this executable to ask the user something; it opens no window.
    if let Some(code) = desktop_ssh_askpass::run_helper_if_requested() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // The WebView receives no updater capability. Native commands below
        // expose the fixed, stateful operations owned by Desktop Update. The
        // plugin's required static config deliberately has no usable key;
        // each check builds an updater from compiled Release/Recovery trust.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .register_uri_scheme_protocol(desktop_renderer_protocol::SCHEME, |ctx, request| {
            desktop_renderer_protocol::handle(ctx.app_handle(), request.uri().path())
        })
        .setup(|app| {
            desktop_log::install(&app.path().app_log_dir()?);
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
            app.manage(desktop_connection::DesktopConnections::default());
            app.manage(desktop_remote::DesktopRemoteState::new(&app_local_data));
            app.manage(desktop_ssh::DesktopSshState::new(
                app.path().app_cache_dir()?,
            ));
            app.manage(DesktopUpdateState::for_build(
                app_local_data.join("desktop-update-receipt.json"),
            ));
            app.manage(desktop_quit::DesktopQuitState::default());
            desktop_app_server_hold::install(app);
            desktop_local_attention::install(app);
            #[cfg(target_os = "macos")]
            macos_menu::install(app)?;
            #[cfg(target_os = "macos")]
            configure_macos_webview_resize(app)?;
            #[cfg(all(target_os = "macos", debug_assertions))]
            watch_development_runner(app);
            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "quit" {
                desktop_quit::request_quit(app);
                return;
            }
            let _ = app.emit("desktop-command", event.id().as_ref());
        })
        .on_window_event(|window, event| {
            #[cfg(target_os = "windows")]
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                // Native close and the custom caption button must cross the same
                // detach boundary, and the same Terminal warning, as explicit Quit.
                api.prevent_close();
                desktop_quit::request_quit(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop_connection::desktop_bootstrap,
            desktop_bootstrap_context,
            desktop_connection::desktop_ensure_app_server,
            desktop_quit::desktop_take_pending_quit,
            desktop_quit::desktop_set_open_terminals,
            desktop_app_server_hold::complete_desktop_quit,
            desktop_app_server_hold::desktop_update_prepare_local,
            desktop_local_attention::desktop_local_attention_snapshot,
            desktop_app_server_hold::desktop_update_finish_local,
            desktop_webview_log::record_desktop_telemetry,
            desktop_webview_log::record_desktop_webview_log,
            prepare_desktop_native_zoom,
            perform_desktop_native_zoom,
            set_desktop_runtime_environment,
            desktop_prepare_project_folder,
            desktop_dismiss_path_warning,
            desktop_support_export::desktop_save_support_export,
            desktop_secrets::desktop_secret_read,
            desktop_secrets::desktop_secret_write,
            desktop_secrets::desktop_secret_delete,
            recover_desktop_runtime_environment,
            desktop_remote::desktop_remote_snapshot,
            desktop_remote::desktop_remote_pair,
            desktop_remote::desktop_remote_use,
            desktop_remote::desktop_remote_cancel,
            desktop_remote::desktop_remote_forget,
            desktop_ssh::desktop_ssh_hosts,
            desktop_ssh::desktop_ssh_add,
            desktop_ssh::desktop_ssh_status,
            desktop_ssh::desktop_ssh_failure,
            desktop_ssh::desktop_ssh_update,
            desktop_ssh::desktop_ssh_stop,
            desktop_ssh::desktop_ssh_prompts,
            desktop_ssh::desktop_ssh_prompt_answer,
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
        remote_computer: preferences.remote().map(|computer| computer.name.clone()),
    })
}

#[cfg(target_os = "macos")]
fn configure_macos_webview_resize(app: &mut tauri::App) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .expect("main desktop window must exist during setup");
    macos_webview_resize::configure(&window)
}

/// Exit code that asks the macOS development runner to start the app again.
#[cfg(all(target_os = "macos", debug_assertions))]
const DEVELOPMENT_RESTART_EXIT_CODE: i32 = 75;

/// Restarts Desktop so a changed bootstrap preference takes effect. The macOS
/// development runner owns the app process, its output, and the frontend
/// server, so there it is asked to relaunch; a self-relaunch would outlive it.
pub(crate) fn restart_desktop(app: &tauri::AppHandle) {
    #[cfg(all(target_os = "macos", debug_assertions))]
    if std::env::var_os("OPENAIDE_DEVELOPMENT_RUNNER_PID").is_some() {
        desktop_log!("desktop_restart_requested owner=development_runner");
        app.exit(DEVELOPMENT_RESTART_EXIT_CODE);
        return;
    }
    desktop_log!("desktop_restart_requested owner=desktop");
    app.restart();
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
    desktop_log!("desktop_development_runner_watch_started");
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
            desktop_log!("desktop_development_runner_watch_completed outcome=parent_ended");
            app_handle.exit(0);
            break;
        }
    });
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
    restart_desktop(&app);
    Ok(())
}

#[tauri::command]
fn recover_desktop_runtime_environment(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<(), String> {
    // Leaving an unreachable paired computer comes before undoing an environment change.
    if desktop_remote::use_this_computer(&state)? {
        restart_desktop(&app);
        return Ok(());
    }
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
    restart_desktop(&app);
    Ok(())
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
