use serde_json::json;

use super::*;

fn http(status: u16, body: &serde_json::Value) -> Vec<u8> {
    format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\n\r\n{body}").into_bytes()
}

#[test]
fn reads_the_result_among_the_messages_delivered_with_it() {
    let devices = json!({ "remoteAccess": "on", "serverName": "studio" });
    let answer = read_answer(
        &http(200, &json!([
            { "jsonrpc": "2.0", "id": "terminal-7", "result": { "result": { "ok": true }, "meta": {} } },
            { "jsonrpc": "2.0", "method": "event", "params": { "payload": { "kind": "deviceCollectionUpdated", "devices": devices } } },
        ])),
        "terminal-7",
    )
    .unwrap();
    assert_eq!(answer.result, json!({ "ok": true }));
    assert_eq!(answer.devices.unwrap().server_name, "studio");

    let alone = read_answer(
        &http(
            200,
            &json!({ "id": "terminal-1", "result": { "result": {} } }),
        ),
        "terminal-1",
    )
    .unwrap();
    assert!(alone.devices.is_none());
}

#[test]
fn names_a_refusal_by_its_code() {
    let refused = read_answer(
        &http(
            200,
            &json!({ "id": "terminal-1", "error": { "error": { "code": "unknownClient", "message": "gone" } } }),
        ),
        "terminal-1",
    );
    assert_eq!(
        refused.err(),
        Some(ClientError::Refused {
            code: "unknownClient".into(),
            message: "gone".into()
        })
    );
    assert!(matches!(
        read_answer(&http(403, &json!({})), "terminal-1"),
        Err(ClientError::Refused { .. })
    ));
}

#[test]
fn an_answer_for_another_request_or_no_answer_is_not_a_result() {
    assert_eq!(
        read_answer(
            &http(200, &json!({ "id": "other", "result": {} })),
            "terminal-1"
        )
        .err(),
        Some(ClientError::Malformed)
    );
    assert_eq!(
        read_answer(b"", "terminal-1").err(),
        Some(ClientError::Unreachable)
    );
    assert_eq!(
        read_answer(b"HTTP/1.1 200 OK\r\n\r\nnot json", "terminal-1").err(),
        Some(ClientError::Malformed)
    );
}

#[test]
fn only_a_loopback_address_is_accepted() {
    assert!(LocalClient::new("http://127.0.0.1:4100/probe", "t").is_ok());
    assert!(LocalClient::new("http://192.0.2.7:4100/probe", "t").is_err());
    assert!(LocalClient::new("https://127.0.0.1:4100/probe", "t").is_err());
}

#[test]
fn sums_running_tasks_wherever_the_projection_reports_them() {
    let settings = json!({ "agents": [{ "runningTaskCount": 2 }, { "other": { "runningTaskCount": 1 } }, {}] });
    assert_eq!(sum_field(&settings, "runningTaskCount"), 3);
}
