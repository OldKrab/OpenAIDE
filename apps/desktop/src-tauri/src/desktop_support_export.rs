use std::path::Path;
use std::time::Duration;

use url::Url;

use crate::desktop_runtime::DesktopBootstrapPreferences;

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

#[cfg(test)]
#[path = "desktop_support_export_tests.rs"]
mod tests;
