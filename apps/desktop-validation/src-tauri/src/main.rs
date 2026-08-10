use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
#[cfg(target_os = "macos")]
use tauri::Manager;
#[cfg(target_os = "macos")]
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

#[cfg(target_os = "macos")]
mod macos_webview_resize;

const HANDOFF_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_HANDOFF_LINE_BYTES: usize = 8 * 1024;
const MIN_AUTH_TOKEN_BYTES: usize = 32;

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
}

struct DesktopState {
    client_instance_id: String,
    connection: Mutex<Option<LocalHttpConnection>>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            client_instance_id: format!("desktop-{}", uuid::Uuid::new_v4()),
            connection: Mutex::new(None),
        }
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(DesktopState::default())
        .setup(|app| {
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
        .invoke_handler(tauri::generate_handler![
            desktop_bootstrap,
            complete_desktop_quit,
            record_desktop_telemetry,
            prepare_desktop_native_zoom,
            perform_desktop_native_zoom
        ])
        .run(tauri::generate_context!())
        .expect("failed to run OpenAIDE desktop validation shell");
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
    let quit = MenuItemBuilder::with_id("quit", "Quit OpenAIDE")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;
    let application = SubmenuBuilder::new(app, "OpenAIDE")
        .items(&[&settings, &PredefinedMenuItem::separator(app)?, &quit])
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
    state: tauri::State<'_, DesktopState>,
) -> Result<DesktopBootstrap, String> {
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
        });
    }

    eprintln!("desktop_app_server_handoff_started");
    let connection = tauri::async_runtime::spawn_blocking(launch_app_server_handoff)
        .await
        .map_err(|_| "App Server handoff task failed".to_string())?
        .map_err(|error| {
            eprintln!("desktop_app_server_handoff_failed stage={}", error.stage);
            error.user_message
        })?;
    eprintln!("desktop_app_server_handoff_completed");
    *state
        .connection
        .lock()
        .map_err(|_| "desktop connection state unavailable")? = Some(connection.clone());
    Ok(DesktopBootstrap {
        client_instance_id: state.client_instance_id.clone(),
        connection,
        platform: std::env::consts::OS,
    })
}

struct HandoffFailure {
    stage: &'static str,
    user_message: String,
}

fn launch_app_server_handoff() -> Result<LocalHttpConnection, HandoffFailure> {
    let binary = required_path("OPENAIDE_APP_SERVER_BIN", "binary_configuration")?;
    let storage_root = required_path("OPENAIDE_STORAGE_ROOT", "storage_configuration")?;
    let runtime_root = required_path("OPENAIDE_RUNTIME_ROOT", "runtime_configuration")?;
    create_directory(&storage_root, "storage_create")?;
    create_directory(&runtime_root, "runtime_create")?;

    let mut child = Command::new(&binary)
        .env("OPENAIDE_APP_SERVER_PROTOCOL", "app-server-handoff")
        .env("OPENAIDE_STORAGE_ROOT", &storage_root)
        .env("OPENAIDE_RUNTIME_ROOT", &runtime_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
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

fn required_path(variable: &str, stage: &'static str) -> Result<PathBuf, HandoffFailure> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .ok_or_else(|| {
            failure(
                stage,
                "Desktop Development is not configured for this checkout.",
            )
        })
}

fn create_directory(path: &std::path::Path, stage: &'static str) -> Result<(), HandoffFailure> {
    std::fs::create_dir_all(path)
        .map_err(|_| failure(stage, "OpenAIDE could not prepare its development storage."))
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
