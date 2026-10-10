//! The label a Remote Device shows for the machine an App Server runs on.

const FALLBACK: &str = "This computer";

#[cfg(unix)]
pub(super) fn host_name() -> String {
    nix::unistd::gethostname()
        .ok()
        .and_then(|name| name.into_string().ok())
        .map(|name| super::pairing_code::clamp_label(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| FALLBACK.to_string())
}

#[cfg(not(unix))]
pub(super) fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .map(|name| super::pairing_code::clamp_label(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| FALLBACK.to_string())
}
