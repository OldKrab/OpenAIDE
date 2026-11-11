use std::time::Instant;

const SERVICE: &str = "dev.openaide.desktop.secrets";

#[tauri::command]
pub(crate) fn desktop_secret_read(key: String) -> Result<Option<String>, String> {
    validate_key(&key)?;
    secret_operation("read", || read_platform_secret(&key))
}

#[tauri::command]
pub(crate) fn desktop_secret_write(key: String, value: String) -> Result<(), String> {
    validate_key(&key)?;
    secret_operation("write", || write_platform_secret(&key, &value))
}

#[tauri::command]
pub(crate) fn desktop_secret_delete(key: String) -> Result<(), String> {
    validate_key(&key)?;
    secret_operation("delete", || delete_platform_secret(&key))
}

fn secret_operation<T>(
    operation: &'static str,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let operation_id = format!("desktop-secret-{}", uuid::Uuid::new_v4());
    let started_at = Instant::now();
    desktop_log!(
        "desktop_secret_operation_started operation_id={operation_id} operation={operation} attempt_count=1"
    );
    let result = run();
    desktop_log!(
        "desktop_secret_operation_completed operation_id={operation_id} operation={operation} outcome={} duration_ms={} attempt_count=1",
        if result.is_ok() { "success" } else { "failure" },
        started_at.elapsed().as_millis(),
    );
    result
}

fn validate_key(key: &str) -> Result<(), String> {
    let valid_prefix = key.starts_with("openaide.agent.") || key.starts_with("openaide.mcp.");
    let valid_chars = key
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && byte != b'/' && byte != b'\\');
    if valid_prefix && valid_chars && key.len() <= 512 {
        Ok(())
    } else {
        Err("Secure storage key is invalid.".to_string())
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn platform_entry(key: &str) -> Result<keyring_core::Entry, String> {
    crate::desktop_keyring::entry(SERVICE, key)
        .map_err(|error| store_failure("open", &error, "Secure storage is unavailable."))
}

/// Logs a credential store failure by class only, and returns the user-facing message.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn store_failure(step: &str, error: &keyring_core::Error, message: &str) -> String {
    desktop_log!(
        "desktop_secret_store_failed step={step} error_class={}",
        crate::desktop_keyring::error_class(error)
    );
    message.to_string()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn read_platform_secret(key: &str) -> Result<Option<String>, String> {
    match platform_entry(key)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(store_failure(
            "read",
            &error,
            "Secure storage could not be read.",
        )),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_platform_secret(_key: &str) -> Result<Option<String>, String> {
    Err("Secure storage is unavailable on this Desktop platform.".to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn write_platform_secret(key: &str, value: &str) -> Result<(), String> {
    platform_entry(key)?
        .set_password(value)
        .map_err(|error| store_failure("write", &error, "Secure storage could not be written."))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn write_platform_secret(_key: &str, _value: &str) -> Result<(), String> {
    Err("Secure storage is unavailable on this Desktop platform.".to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn delete_platform_secret(key: &str) -> Result<(), String> {
    match platform_entry(key)?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(store_failure(
            "delete",
            &error,
            "Secure storage could not be deleted.",
        )),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn delete_platform_secret(_key: &str) -> Result<(), String> {
    Err("Secure storage is unavailable on this Desktop platform.".to_string())
}

#[cfg(test)]
#[path = "desktop_secrets_tests.rs"]
mod tests;
