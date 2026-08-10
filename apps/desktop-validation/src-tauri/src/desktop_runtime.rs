use std::ffi::OsString;
use std::path::{Path, PathBuf};

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
    /// stable, platform-native defaults derived from Tauri's application identity.
    pub(crate) fn resolve(app_local_data: &Path, executable_dir: &Path) -> Self {
        Self::resolve_with_overrides(
            app_local_data,
            executable_dir,
            RuntimePathOverrides::from_environment(),
        )
    }

    fn resolve_with_overrides(
        app_local_data: &Path,
        executable_dir: &Path,
        overrides: RuntimePathOverrides,
    ) -> Self {
        Self {
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
        }
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

#[cfg(test)]
#[path = "desktop_runtime_tests.rs"]
mod tests;
