use std::path::Path;

use crate::desktop_runtime::DesktopBootstrapPreferences;
use crate::desktop_support_export::{
    download_support_export, remember_export_directory, support_export_download_url,
    validate_export_label,
};

#[test]
fn support_export_download_returns_bytes_without_global_tls_initialization() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    // The updater enables reqwest's provider-free Rustls backend. Export must
    // still construct its own client before any other Desktop operation runs,
    // even though its authenticated download uses loopback HTTP rather than TLS.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = support_export_download_url(
        &format!("http://{}/probe", listener.local_addr().unwrap()),
        "desktop-client",
        "export-handle",
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "download did not connect");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("download listener failed: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with(
            "GET /download?clientInstanceId=desktop-client&fileHandleId=export-handle HTTP/1.1\r\n"
        ));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-token\r\n")
        );
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nPK-test")
            .unwrap();
    });

    let bytes = tauri::async_runtime::block_on(download_support_export(url, "fixture-token"))
        .expect("the download must return instead of panicking during client construction");
    assert_eq!(bytes, b"PK-test");
    server.join().unwrap();
}

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
