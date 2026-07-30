use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};

const MAX_HANDOFF_LINE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalHttpConnection {
    kind: String,
    endpoint_url: String,
    auth_token: String,
}

#[derive(Default)]
struct PrototypeState {
    connection: Mutex<Option<LocalHttpConnection>>,
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(PrototypeState::default())
        .setup(|app| {
            install_native_menu(app)?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            command => {
                let _ = app.emit("desktop-prototype-command", command);
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_server_connection,
            record_folder_picker_result
        ])
        .run(tauri::generate_context!())
        .expect("failed to run OpenAIDE desktop prototype");
}

fn install_native_menu(app: &mut tauri::App) -> tauri::Result<()> {
    let new_task = MenuItemBuilder::with_id("new-task", "New Task")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let choose_folder = MenuItemBuilder::with_id("choose-folder", "Choose Project Folder…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let reload = MenuItemBuilder::with_id("reload", "Reload")
        .accelerator("CmdOrCtrl+R")
        .build(app)?;
    let notification =
        MenuItemBuilder::with_id("test-notification", "Send Test Notification").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let file = SubmenuBuilder::new(app, "File")
        .items(&[&new_task, &choose_folder, &settings, &quit])
        .build()?;
    let view = SubmenuBuilder::new(app, "View").items(&[&reload]).build()?;
    let help = SubmenuBuilder::new(app, "Help")
        .items(&[&notification])
        .build()?;
    let menu = MenuBuilder::new(app)
        .items(&[&file, &view, &help])
        .build()?;
    app.set_menu(menu)?;
    Ok(())
}

#[tauri::command]
async fn app_server_connection(
    state: tauri::State<'_, PrototypeState>,
) -> Result<LocalHttpConnection, String> {
    if let Some(connection) = state
        .connection
        .lock()
        .map_err(|_| "prototype state lock poisoned")?
        .clone()
    {
        return Ok(connection);
    }

    let connection = tauri::async_runtime::spawn_blocking(launch_app_server_handoff)
        .await
        .map_err(|error| format!("App Server handoff task failed: {error}"))??;
    *state
        .connection
        .lock()
        .map_err(|_| "prototype state lock poisoned")? = Some(connection.clone());
    Ok(connection)
}

fn launch_app_server_handoff() -> Result<LocalHttpConnection, String> {
    let binary = app_server_binary()?;
    let prototype_root = prototype_root()?;
    let storage_root = prototype_root.join("state");
    let runtime_root = prototype_root.join("runtime");
    std::fs::create_dir_all(&storage_root)
        .map_err(|error| format!("failed to create prototype state root: {error}"))?;
    std::fs::create_dir_all(&runtime_root)
        .map_err(|error| format!("failed to create prototype runtime root: {error}"))?;

    let mut child = Command::new(&binary)
        .env("OPENAIDE_APP_SERVER_PROTOCOL", "app-server-handoff")
        .env("OPENAIDE_STORAGE_ROOT", &storage_root)
        .env("OPENAIDE_RUNTIME_ROOT", &runtime_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("failed to launch {}: {error}", binary.display()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "App Server handoff has no stdout".to_string())?;
    let mut line = String::new();
    BufReader::new(stdout)
        .take(MAX_HANDOFF_LINE_BYTES as u64)
        .read_line(&mut line)
        .map_err(|error| format!("failed to read App Server handoff: {error}"))?;
    if line.len() >= MAX_HANDOFF_LINE_BYTES || line.trim().is_empty() {
        let _ = child.kill();
        return Err("App Server returned invalid handoff output".to_string());
    }

    let connection: LocalHttpConnection = serde_json::from_str(line.trim())
        .map_err(|error| format!("invalid App Server handoff JSON: {error}"))?;
    validate_connection(&connection)?;
    // Reap both the long-lived launch process and a short-lived attach helper
    // without making the Tauri window the App Server lifetime authority.
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(connection)
}

fn app_server_binary() -> Result<PathBuf, String> {
    let value = std::env::var_os("OPENAIDE_APP_SERVER_BIN")
        .ok_or_else(|| "OPENAIDE_APP_SERVER_BIN is not configured".to_string())?;
    let path = PathBuf::from(value);
    if !path.is_file() {
        return Err(format!(
            "App Server binary does not exist: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn prototype_root() -> Result<PathBuf, String> {
    let value = std::env::var_os("OPENAIDE_DESKTOP_PROTOTYPE_ROOT")
        .ok_or_else(|| "OPENAIDE_DESKTOP_PROTOTYPE_ROOT is not configured".to_string())?;
    Ok(PathBuf::from(value))
}

fn validate_connection(connection: &LocalHttpConnection) -> Result<(), String> {
    if connection.kind != "localHttp" || connection.auth_token.is_empty() {
        return Err("App Server returned an unsupported handoff".to_string());
    }
    if !is_loopback_probe_endpoint(&connection.endpoint_url) {
        return Err("App Server returned a non-loopback endpoint".to_string());
    }
    Ok(())
}

fn is_loopback_probe_endpoint(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("http://") else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    let host = authority
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(authority);
    path == "probe" && matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

#[tauri::command]
fn record_folder_picker_result(selected: bool) {
    eprintln!("desktop prototype folder picker completed; selected={selected}");
}
