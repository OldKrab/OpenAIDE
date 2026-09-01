use std::path::Path;

use crate::desktop_runtime::DesktopBootstrapPreferences;
use crate::desktop_support_export::{
    remember_export_directory, support_export_download_url, validate_export_label,
};

#[test]
fn support_export_download_is_bound_to_the_creating_desktop_client() {
    let url = support_export_download_url(
        "http://127.0.0.1:5574/probe",
        "desktop-client",
        "export-handle",
    )
    .unwrap();

    assert_eq!(
        url.as_str(),
        "http://127.0.0.1:5574/download?clientInstanceId=desktop-client&fileHandleId=export-handle"
    );
}

#[test]
fn support_export_filename_must_be_one_zip_filename() {
    assert!(validate_export_label("openaide-support-123.zip").is_ok());
    for invalid in [
        "",
        ".",
        "../support.zip",
        "folder/support.zip",
        r"folder\support.zip",
        "support.txt",
    ] {
        assert!(
            validate_export_label(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn successful_export_remembers_only_its_parent_directory() {
    let mut preferences = DesktopBootstrapPreferences::default();

    remember_export_directory(&mut preferences, Path::new("/chosen/reports/custom.zip"));

    assert_eq!(
        preferences.support_export_directory.as_deref(),
        Some(Path::new("/chosen/reports"))
    );
}
