use super::rotating_file::{RotatingLogFile, RotationPolicy};
use super::sanitize_value;
use serde_json::json;

#[test]
fn sanitizes_sensitive_runtime_log_fields() {
    let value = sanitize_value(json!({
        "method": "task.create",
        "error": "arbitrary-private-detail Cannot read /home/user/project/file.txt with token abc",
        "nested": { "path": "/workspace/app" },
    }));

    let text = value.to_string();
    assert!(text.contains("task.create"));
    assert!(!text.contains("arbitrary-private-detail"));
    assert!(!text.contains("abc"));
    assert!(!text.contains("/home/user"));
    assert!(!text.contains("/workspace/app"));
    assert!(!text.contains("token"));
}

#[test]
fn rotating_log_keeps_only_the_configured_complete_generations() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("runtime.jsonl");
    let mut log = RotatingLogFile::open(
        path.clone(),
        RotationPolicy {
            max_bytes: 10,
            file_count: 4,
        },
    )
    .unwrap();

    for index in 0..10 {
        log.append_line(&format!("{index:04}")).unwrap();
    }

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "0008\n0009\n");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("runtime.jsonl.1")).unwrap(),
        "0006\n0007\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("runtime.jsonl.2")).unwrap(),
        "0004\n0005\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("runtime.jsonl.3")).unwrap(),
        "0002\n0003\n"
    );
    assert!(!tmp.path().join("runtime.jsonl.4").exists());
}

#[test]
fn rotating_log_moves_an_existing_oversized_file_before_append() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("runtime.jsonl");
    std::fs::write(&path, "legacy-record\n").unwrap();

    let mut log = RotatingLogFile::open(
        path.clone(),
        RotationPolicy {
            max_bytes: 8,
            file_count: 4,
        },
    )
    .unwrap();
    log.append_line("new").unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("runtime.jsonl.1")).unwrap(),
        "legacy-record\n"
    );
}

#[test]
fn rotating_log_rejects_a_record_larger_than_the_file_limit() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("runtime.jsonl");
    let mut log = RotatingLogFile::open(
        path.clone(),
        RotationPolicy {
            max_bytes: 8,
            file_count: 4,
        },
    )
    .unwrap();

    assert_eq!(
        log.append_line("record-too-large").unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert_eq!(std::fs::metadata(path).unwrap().len(), 0);
}
