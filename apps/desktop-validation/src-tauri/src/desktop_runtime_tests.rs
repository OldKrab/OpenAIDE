use super::{APP_SERVER_BINARY_NAME, DesktopRuntimePaths, RuntimePathOverrides};
use std::ffi::OsString;
use std::path::Path;

#[test]
fn installed_runtime_uses_platform_native_application_paths() {
    let paths = DesktopRuntimePaths::resolve_with_overrides(
        Path::new("/native/app-data"),
        Path::new("/installed/bin"),
        RuntimePathOverrides {
            app_server_binary: None,
            storage_root: None,
            runtime_root: None,
        },
    );

    assert_eq!(
        paths.app_server_binary,
        Path::new("/installed/bin").join(APP_SERVER_BINARY_NAME)
    );
    assert_eq!(paths.storage_root, Path::new("/native/app-data/state"));
    assert_eq!(paths.runtime_root, Path::new("/native/app-data/runtime"));
}

#[test]
fn explicit_development_paths_override_every_installed_default() {
    let paths = DesktopRuntimePaths::resolve_with_overrides(
        Path::new("/native/app-data"),
        Path::new("/installed/bin"),
        RuntimePathOverrides {
            app_server_binary: Some(OsString::from("/checkout/app-server")),
            storage_root: Some(OsString::from("/checkout/state")),
            runtime_root: Some(OsString::from("/checkout/runtime")),
        },
    );

    assert_eq!(paths.app_server_binary, Path::new("/checkout/app-server"));
    assert_eq!(paths.storage_root, Path::new("/checkout/state"));
    assert_eq!(paths.runtime_root, Path::new("/checkout/runtime"));
}
