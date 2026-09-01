use super::{
    APP_SERVER_BINARY_NAME, DesktopBootstrapPreferences, DesktopRuntimeEnvironment,
    DesktopRuntimePaths, RuntimePathOverrides, decode_wsl_list, is_docker_managed_distro,
    prepare_project_path,
};
use std::ffi::OsString;
use std::path::Path;

#[test]
fn installed_runtime_uses_app_server_next_to_the_desktop_executable() {
    let paths = DesktopRuntimePaths::resolve_with_overrides(
        Path::new("/native/app-data"),
        Path::new("/installed/bin/openaide-desktop"),
        RuntimePathOverrides {
            app_server_binary: None,
            storage_root: None,
            runtime_root: None,
        },
    )
    .expect("installed executable path should resolve");

    assert_eq!(
        paths.app_server_binary,
        Path::new("/installed/bin").join(APP_SERVER_BINARY_NAME)
    );
    assert_eq!(paths.storage_root, Path::new("/native/app-data/state"));
    assert_eq!(paths.runtime_root, Path::new("/native/app-data/runtime"));
}

#[test]
fn wsl_catalog_ignores_docker_managed_distros_and_decodes_windows_output() {
    let utf16 = "Ubuntu\r\nDocker-Desktop\r\nDebian\r\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();

    assert_eq!(decode_wsl_list(&utf16), vec!["Ubuntu", "Debian"]);
    assert!(is_docker_managed_distro("docker-desktop-data"));
    assert!(!is_docker_managed_distro("Ubuntu"));
}

#[test]
fn bootstrap_preferences_keep_backend_os_data_separate() {
    let preferences = DesktopBootstrapPreferences {
        environment: DesktopRuntimeEnvironment::Wsl {
            distro: "Ubuntu".to_string(),
        },
        previous_environment: Some(DesktopRuntimeEnvironment::Native),
        dismissed_path_warnings: vec!["windows-folder-in-wsl".to_string()],
        support_export_directory: None,
    };
    let encoded = serde_json::to_string(&preferences).expect("preferences should serialize");
    let decoded: DesktopBootstrapPreferences =
        serde_json::from_str(&encoded).expect("preferences should deserialize");

    assert_eq!(decoded, preferences);
    assert!(!encoded.contains("storage"));
    assert!(!encoded.contains("projects"));
}

#[test]
fn wsl_native_unc_path_becomes_linux_native_before_project_add() {
    let prepared = prepare_project_path(
        r"\\wsl.localhost\Ubuntu\home\developer\project",
        &DesktopRuntimeEnvironment::Wsl {
            distro: "Ubuntu".to_string(),
        },
        &[],
        |_| panic!("native WSL path should not need wslpath"),
    )
    .expect("WSL UNC path should translate");

    assert_eq!(prepared.path, "/home/developer/project");
    assert_eq!(prepared.label, "project");
    assert!(prepared.warning.is_none());
}

#[test]
fn windows_folder_in_wsl_is_allowed_with_a_warning() {
    let prepared = prepare_project_path(
        r"C:\work\project",
        &DesktopRuntimeEnvironment::Wsl {
            distro: "Ubuntu".to_string(),
        },
        &[],
        |path| {
            assert_eq!(path, r"C:\work\project");
            Ok("/mnt/c/work/project".to_string())
        },
    )
    .expect("mounted Windows path should remain usable");

    assert_eq!(prepared.path, "/mnt/c/work/project");
    assert_eq!(prepared.warning.unwrap().key, "windows-folder-in-wsl");
}

#[test]
fn wsl_folder_in_windows_is_allowed_with_a_warning() {
    let prepared = prepare_project_path(
        r"\\wsl.localhost\Ubuntu\home\developer\project",
        &DesktopRuntimeEnvironment::Native,
        &[],
        |_| panic!("Windows mode keeps the UNC path"),
    )
    .expect("Windows should accept a WSL UNC path");

    assert_eq!(
        prepared.path,
        r"\\wsl.localhost\Ubuntu\home\developer\project"
    );
    assert_eq!(prepared.warning.unwrap().key, "wsl-folder-in-windows");
}

#[test]
fn explicit_development_paths_override_every_installed_default() {
    let paths = DesktopRuntimePaths::resolve_with_overrides(
        Path::new("/native/app-data"),
        Path::new("/installed/bin/openaide-desktop"),
        RuntimePathOverrides {
            app_server_binary: Some(OsString::from("/checkout/app-server")),
            storage_root: Some(OsString::from("/checkout/state")),
            runtime_root: Some(OsString::from("/checkout/runtime")),
        },
    )
    .expect("development executable path should resolve");

    assert_eq!(paths.app_server_binary, Path::new("/checkout/app-server"));
    assert_eq!(paths.storage_root, Path::new("/checkout/state"));
    assert_eq!(paths.runtime_root, Path::new("/checkout/runtime"));
}
