use std::collections::BTreeMap;

use super::webview_log_line;

fn fields(entries: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

#[test]
fn writes_identifier_and_number_fields() {
    let line = webview_log_line(
        "task_attention_notification",
        &fields(&[
            ("outcome", "shown".into()),
            ("event_id", "request:permission-7:2b1f".into()),
            ("count", 3.into()),
        ]),
    );

    assert_eq!(
        line,
        "task_attention_notification count=3 event_id=request:permission-7:2b1f outcome=shown"
    );
}

#[test]
fn replaces_free_text_and_line_breaks() {
    let line = webview_log_line(
        "desktop_notification_shown\nforged=line",
        &fields(&[
            ("title", "Fix the build".into()),
            ("bad key", "value".into()),
            ("nested", serde_json::json!({ "a": 1 })),
        ]),
    );

    assert_eq!(line, "invalid invalid=value nested=invalid title=invalid");
}

#[test]
fn bounds_the_number_of_fields() {
    let many: Vec<(String, serde_json::Value)> = (0..20)
        .map(|index| (format!("f{index:02}"), index.into()))
        .collect();
    let line = webview_log_line("event", &many.into_iter().collect());

    assert!(line.ends_with(" dropped_fields=8"));
    assert_eq!(line.matches('=').count(), 13);
}
