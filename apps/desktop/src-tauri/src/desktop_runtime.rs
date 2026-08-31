use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const APP_SERVER_BINARY_NAME: &str = if cfg!(windows) {
    "openaide-app-server.exe"
} else {
    "openaide-app-server"
};

/// Paths owned by one installed Desktop instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesktopRuntimePaths {
    pub(crate) app_server_binary: PathBuf,
    pub(crate) storage_root: PathBuf,
    pub(crate) runtime_root: PathBuf,
}

impl DesktopRuntimePaths {
    /// Keeps development/test overrides explicit while giving installed bundles
    /// stable defaults derived from the app data and installed executable paths.
    pub(crate) fn resolve(app_local_data: &Path, executable_path: &Path) -> io::Result<Self> {
        Self::resolve_with_overrides(
            app_local_data,
            executable_path,
            RuntimePathOverrides::from_environment(),
        )
    }

    fn resolve_with_overrides(
        app_local_data: &Path,
        executable_path: &Path,
        overrides: RuntimePathOverrides,
    ) -> io::Result<Self> {
        let executable_dir = executable_path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "desktop executable path has no parent directory",
            )
        })?;
        Ok(Self {
            app_server_binary: overrides
                .app_server_binary
                .map(PathBuf::from)
                .unwrap_or_else(|| executable_dir.join(APP_SERVER_BINARY_NAME)),
            storage_root: overrides
                .storage_root
                .map(PathBuf::from)
                .unwrap_or_else(|| app_local_data.join("state")),
            runtime_root: overrides
                .runtime_root
                .map(PathBuf::from)
                .unwrap_or_else(|| app_local_data.join("runtime")),
        })
    }
}

struct RuntimePathOverrides {
    app_server_binary: Option<OsString>,
    storage_root: Option<OsString>,
    runtime_root: Option<OsString>,
}

impl RuntimePathOverrides {
    fn from_environment() -> Self {
        Self {
            app_server_binary: std::env::var_os("OPENAIDE_APP_SERVER_BIN"),
            storage_root: std::env::var_os("OPENAIDE_STORAGE_ROOT"),
            runtime_root: std::env::var_os("OPENAIDE_RUNTIME_ROOT"),
        }
    }
}

/// Selects the operating-system boundary that owns App Server state and Agents.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum DesktopRuntimeEnvironment {
    #[default]
    Native,
    Wsl {
        distro: String,
    },
}

/// Shell bootstrap preferences intentionally exclude product data and credentials.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopBootstrapPreferences {
    pub(crate) environment: DesktopRuntimeEnvironment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) previous_environment: Option<DesktopRuntimeEnvironment>,
    #[serde(default)]
    pub(crate) dismissed_path_warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedProjectFolder {
    pub(crate) label: String,
    pub(crate) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) warning: Option<ProjectPathWarning>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ProjectPathWarning {
    pub(crate) key: &'static str,
    pub(crate) message: &'static str,
}

pub(crate) fn is_docker_managed_distro(name: &str) -> bool {
    name.to_ascii_lowercase().contains("docker")
}

/// `wsl.exe --list --quiet` is UTF-16LE in Windows PowerShell and may be UTF-8
/// when invoked through compatibility layers, so accept both representations.
pub(crate) fn decode_wsl_list(bytes: &[u8]) -> Vec<String> {
    let decoded = if bytes.len() >= 2 && (bytes[1] == 0 || bytes.starts_with(&[0xff, 0xfe])) {
        let start = usize::from(bytes.starts_with(&[0xff, 0xfe])) * 2;
        let words = bytes[start..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    decoded
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty() && !is_docker_managed_distro(name))
        .map(str::to_string)
        .collect()
}

pub(crate) fn prepare_project_path(
    selected: &str,
    environment: &DesktopRuntimeEnvironment,
    dismissed_warnings: &[String],
    translate_windows_path: impl FnOnce(&str) -> io::Result<String>,
) -> io::Result<PreparedProjectFolder> {
    let wsl_unc = parse_wsl_unc_path(selected);
    let (path, warning) = match environment {
        DesktopRuntimeEnvironment::Native => (
            selected.to_string(),
            wsl_unc.and_then(|_| {
                warning(
                    "wsl-folder-in-windows",
                    "This WSL folder will usually work better in the matching WSL environment.",
                    dismissed_warnings,
                )
            }),
        ),
        DesktopRuntimeEnvironment::Wsl { distro } => {
            if let Some((selected_distro, linux_path)) = wsl_unc {
                if selected_distro.eq_ignore_ascii_case(distro) {
                    (linux_path, None)
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "The selected folder belongs to a different WSL distribution.",
                    ));
                }
            } else if looks_like_windows_path(selected) {
                (
                    translate_windows_path(selected)?.trim().to_string(),
                    warning(
                        "windows-folder-in-wsl",
                        "This Windows folder may work better in the Windows environment.",
                        dismissed_warnings,
                    ),
                )
            } else {
                (selected.to_string(), None)
            }
        }
    };
    let label = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(&path)
        .to_string();
    Ok(PreparedProjectFolder {
        label,
        path,
        warning,
    })
}

fn warning(
    key: &'static str,
    message: &'static str,
    dismissed: &[String],
) -> Option<ProjectPathWarning> {
    (!dismissed.iter().any(|value| value == key)).then_some(ProjectPathWarning { key, message })
}

fn looks_like_windows_path(path: &str) -> bool {
    path.as_bytes().get(1) == Some(&b':') || path.starts_with(r"\\")
}

fn parse_wsl_unc_path(path: &str) -> Option<(String, String)> {
    let normalized = path.replace('/', "\\");
    let rest = normalized
        .strip_prefix(r"\\wsl.localhost\")
        .or_else(|| normalized.strip_prefix(r"\\wsl$\"))?;
    let (distro, tail) = rest.split_once('\\').unwrap_or((rest, ""));
    let linux_path = format!("/{}", tail.replace('\\', "/").trim_start_matches('/'));
    Some((distro.to_string(), linux_path))
}

#[cfg(test)]
#[path = "desktop_runtime_tests.rs"]
mod tests;
