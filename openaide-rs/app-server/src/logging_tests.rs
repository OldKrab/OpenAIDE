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
