use std::collections::BTreeMap;

use openaide_app_server_protocol::diagnostics::{ClientDiagnosticEvent, ClientDiagnosticValue};
use serde_json::json;

use super::{record_client_events, MAX_EVENTS};
use crate::logging::capture_test_logs;

fn event(name: &str, fields: &[(&str, ClientDiagnosticValue)]) -> ClientDiagnosticEvent {
    ClientDiagnosticEvent {
        event: name.to_string(),
        at_ms: 1_000,
        fields: fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn logged(capture: &crate::logging::TestLogCapture, connection: &str) -> Vec<serde_json::Value> {
    capture
        .snapshot()
        .into_iter()
        .filter(|line| line["fields"]["connection_id"] == json!(connection))
        .collect()
}

#[test]
fn a_task_switch_event_is_written_with_its_metadata() {
    let capture = capture_test_logs();
    let recorded = record_client_events(
        "conn-switch",
        vec![event(
            "task_route_open_completed",
            &[
                (
                    "task_id",
                    ClientDiagnosticValue::Word("task_814d26fd".to_string()),
                ),
                ("duration_ms", ClientDiagnosticValue::Count(274)),
                ("accepted", ClientDiagnosticValue::Flag(true)),
            ],
        )],
    );

    assert_eq!(recorded, 1);
    let lines = logged(&capture, "conn-switch");
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0]["event"],
        json!("frontend_task_route_open_completed")
    );
    assert_eq!(lines[0]["fields"]["task_id"], json!("task_814d26fd"));
    assert_eq!(lines[0]["fields"]["duration_ms"], json!(274));
    assert_eq!(lines[0]["fields"]["accepted"], json!(true));
    assert_eq!(lines[0]["fields"]["client_at_ms"], json!(1_000));
}

#[test]
fn free_text_and_malformed_names_never_reach_the_log() {
    let capture = capture_test_logs();
    let recorded = record_client_events(
        "conn-text",
        vec![
            event(
                "task_route_open_failed",
                &[
                    (
                        "error_kind",
                        ClientDiagnosticValue::Word("could not open /home/user/x".to_string()),
                    ),
                    ("Bad Key", ClientDiagnosticValue::Count(1)),
                    (
                        "error_code",
                        ClientDiagnosticValue::Word("not_found".to_string()),
                    ),
                ],
            ),
            event("Not An Event", &[]),
        ],
    );

    assert_eq!(recorded, 1);
    let lines = logged(&capture, "conn-text");
    let written = lines
        .iter()
        .find(|line| line["event"] == json!("frontend_task_route_open_failed"))
        .unwrap();
    assert!(written["fields"].get("error_kind").is_none());
    assert!(written["fields"].get("Bad Key").is_none());
    assert_eq!(written["fields"]["error_code"], json!("not_found"));
    let dropped = lines
        .iter()
        .find(|line| line["event"] == json!("frontend_events_dropped"))
        .unwrap();
    assert_eq!(dropped["fields"]["dropped_count"], json!(1));
}

#[test]
fn a_batch_past_the_limit_is_cut() {
    let _capture = capture_test_logs();
    let events = (0..MAX_EVENTS + 3)
        .map(|_| event("task_state_baseline_ready", &[]))
        .collect();
    assert_eq!(
        record_client_events("conn-batch", events),
        MAX_EVENTS as u32
    );
}
