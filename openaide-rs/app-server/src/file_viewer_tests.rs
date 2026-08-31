use super::*;
use openaide_app_server_protocol::ids::ClientInstanceId;

fn client() -> ClientInstanceId {
    ClientInstanceId::from("client-file-viewer")
}

#[test]
fn open_returns_utf8_source_snapshot_and_handle_for_retry() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    let registry = FileViewerRegistry::new();

    let snapshot = registry.open(&client(), dir.path().to_str().unwrap(), "main.rs", Some(1));

    assert_eq!(snapshot.kind, FileViewerKind::Source);
    assert_eq!(snapshot.basename, "main.rs");
    assert_eq!(snapshot.text.as_deref(), Some("fn main() {}\n"));
    assert_eq!(snapshot.focus_line, Some(1));
    assert!(!snapshot.handle.as_str().is_empty());

    std::fs::write(&path, "fn updated() {}\n").unwrap();
    let refreshed = registry.refresh(&client(), &snapshot.handle, None);
    assert_eq!(refreshed.text.as_deref(), Some("fn updated() {}\n"));
    assert_eq!(refreshed.handle, snapshot.handle);
}

#[test]
fn open_keeps_a_failed_tab_with_classified_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let registry = FileViewerRegistry::new();

    let snapshot = registry.open(&client(), dir.path().to_str().unwrap(), "missing.env", None);

    assert_eq!(snapshot.kind, FileViewerKind::Error);
    assert_eq!(snapshot.error, Some(FileViewerError::NotFound));
    assert_eq!(snapshot.basename, "missing.env");
    assert!(!snapshot.handle.as_str().is_empty());
}

#[test]
fn relative_href_from_handle_opens_sibling_without_a_client_path() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "See [notes](notes.md)\n").unwrap();
    std::fs::write(dir.path().join("notes.md"), "# Notes\n").unwrap();
    let registry = FileViewerRegistry::new();
    let readme = registry.open(&client(), dir.path().to_str().unwrap(), "README.md", None);

    let notes = registry.open_from_handle(&client(), &readme.handle, "notes.md");

    assert_eq!(notes.kind, FileViewerKind::Markdown);
    assert_eq!(notes.basename, "notes.md");
    assert_eq!(notes.text.as_deref(), Some("# Notes\n"));
    assert_ne!(notes.handle, readme.handle);
}
