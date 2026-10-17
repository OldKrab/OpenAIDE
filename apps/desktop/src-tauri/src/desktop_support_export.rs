use std::path::Path;
use std::time::{Duration, Instant};

use tauri_plugin_dialog::DialogExt;
use url::Url;

use crate::DesktopState;
use crate::desktop_runtime::{DesktopBootstrapPreferences, write_bootstrap_preferences};

pub(crate) fn validate_export_label(label: &str) -> Result<(), String> {
    if label.is_empty()
        || label == "."
        || label == ".."
        || label.contains(['/', '\\'])
        || !label.to_ascii_lowercase().ends_with(".zip")
    {
        return Err("The support export filename is invalid.".to_string());
    }
    Ok(())
}

pub(crate) fn support_export_download_url(
    endpoint_url: &str,
    client_instance_id: &str,
    file_handle_id: &str,
) -> Result<Url, String> {
    let mut url = Url::parse(endpoint_url)
        .map_err(|_| "The App Server download endpoint is invalid.".to_string())?;
    let path = url.path().trim_end_matches('/');
    let Some(prefix) = path.strip_suffix("/probe") else {
        return Err("The App Server download endpoint is invalid.".to_string());
    };
    url.set_path(&format!("{prefix}/download"));
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("clientInstanceId", client_instance_id)
        .append_pair("fileHandleId", file_handle_id);
    Ok(url)
}

pub(crate) async fn download_support_export(url: Url, auth_token: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()
        .map_err(|_| "Unable to prepare the support export download.".to_string())?;
    let response = client
        .get(url)
        .bearer_auth(auth_token)
        .send()
        .await
        .map_err(|_| "Unable to download the support export.".to_string())?;
    if !response.status().is_success() {
        return Err("Unable to download the support export.".to_string());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "Unable to download the support export.".to_string())?;
    Ok(bytes.to_vec())
}

pub(crate) fn remember_export_directory(
    preferences: &mut DesktopBootstrapPreferences,
    destination: &Path,
) {
    preferences.support_export_directory = destination.parent().map(Path::to_path_buf);
}

/// Saves an App Server-owned export where the user chooses, through the
/// endpoint this Desktop is connected to.
#[tauri::command]
pub(crate) async fn desktop_save_support_export(
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
    desktop_log!("desktop_support_export_save_started operation_id={operation_id} attempt_count=1");

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
        desktop_log!(
            "desktop_support_export_save_completed operation_id={operation_id} outcome=cancelled duration_ms={} attempt_count=1",
            started_at.elapsed().as_millis()
        );
        return Ok(false);
    };
    let destination = match destination.into_path() {
        Ok(destination) => destination,
        Err(_) => {
            desktop_log!(
                "desktop_support_export_save_completed operation_id={operation_id} outcome=failure error_kind=invalid_destination duration_ms={} attempt_count=1",
                started_at.elapsed().as_millis()
            );
            return Err("The selected support export path is invalid.".to_string());
        }
    };
    let bytes = match download_support_export(url, &connection.auth_token).await {
        Ok(bytes) => bytes,
        Err(error) => {
            desktop_log!(
                "desktop_support_export_save_completed operation_id={operation_id} outcome=failure error_kind=download duration_ms={} attempt_count=1",
                started_at.elapsed().as_millis()
            );
            return Err(error);
        }
    };
    if std::fs::write(&destination, bytes).is_err() {
        desktop_log!(
            "desktop_support_export_save_completed operation_id={operation_id} outcome=failure error_kind=file_write duration_ms={} attempt_count=1",
            started_at.elapsed().as_millis()
        );
        return Err("Unable to save the support export to the selected file.".to_string());
    }

    match state.preferences.lock() {
        Ok(mut preferences) => {
            remember_export_directory(&mut preferences, &destination);
            if write_bootstrap_preferences(&state.preferences_path, &preferences).is_err() {
                desktop_log!(
                    "desktop_support_export_directory_remember_failed operation_id={operation_id} error_kind=preference_write"
                );
            }
        }
        Err(_) => desktop_log!(
            "desktop_support_export_directory_remember_failed operation_id={operation_id} error_kind=preference_lock"
        ),
    }
    desktop_log!(
        "desktop_support_export_save_completed operation_id={operation_id} outcome=success duration_ms={} attempt_count=1",
        started_at.elapsed().as_millis()
    );
    Ok(true)
}

#[cfg(test)]
#[path = "desktop_support_export_tests.rs"]
mod tests;
