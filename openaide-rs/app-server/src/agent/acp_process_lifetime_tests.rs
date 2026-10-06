// timing-file: mocked — every test runs on tokio's paused clock.
use std::time::Duration;

use agent_client_protocol::RawJsonRpcMessage;
use serde_json::json;

use super::{AcpProcessLifetime, ProcessIdleTimeouts};

const SHORT: Duration = Duration::from_secs(10);
const LONG: Duration = Duration::from_secs(100);

fn lifetime() -> AcpProcessLifetime {
    AcpProcessLifetime::new(ProcessIdleTimeouts {
        short: SHORT,
        long: LONG,
    })
}

fn message(value: serde_json::Value) -> RawJsonRpcMessage {
    serde_json::from_value(value).expect("JSON-RPC message")
}

fn request(id: u64, method: &str) -> RawJsonRpcMessage {
    message(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": {} }))
}

fn response(id: u64) -> RawJsonRpcMessage {
    message(json!({ "jsonrpc": "2.0", "id": id, "result": {} }))
}

fn notification(method: &str) -> RawJsonRpcMessage {
    message(json!({ "jsonrpc": "2.0", "method": method, "params": {} }))
}

/// Time is paused, so the elapsed virtual time is exactly the deadline that fired.
async fn expires_after(lifetime: &AcpProcessLifetime) -> Duration {
    let started = tokio::time::Instant::now();
    lifetime.expired().await;
    started.elapsed()
}

/// The periodic Native Session catalog refresh runs every five minutes. List-only
/// retention must outlast it, or every refresh cold-starts each Agent process.
#[test]
fn list_only_retention_outlasts_the_catalog_refresh_interval() {
    let timeouts = ProcessIdleTimeouts::default();
    assert!(timeouts.short > Duration::from_secs(5 * 60));
    assert!(timeouts.long > timeouts.short);
}

#[tokio::test(start_paused = true)]
async fn untouched_process_expires_after_short_retention() {
    let lifetime = lifetime();
    assert_eq!(expires_after(&lifetime).await, SHORT);
    assert!(lifetime.is_stopping());
    assert!(lifetime.acquire().is_none());
}

#[tokio::test(start_paused = true)]
async fn read_only_requests_keep_short_retention() {
    for method in [
        "initialize",
        "session/list",
        "session/resume",
        "session/load",
    ] {
        let lifetime = lifetime();
        lifetime.observe(&request(1, method), true);
        lifetime.observe(&response(1), false);
        assert_eq!(expires_after(&lifetime).await, SHORT, "{method}");
    }
}

#[tokio::test(start_paused = true)]
async fn session_work_promotes_to_long_retention() {
    for method in [
        "session/new",
        "session/prompt",
        "session/fork",
        "session/set_config_option",
        "session/set_mode",
        "session/set_model",
        "session/delete",
    ] {
        let lifetime = lifetime();
        lifetime.observe(&request(1, method), true);
        assert_eq!(expires_after(&lifetime).await, LONG, "{method}");
    }
}

#[tokio::test(start_paused = true)]
async fn agent_requests_for_session_work_do_not_promote() {
    let lifetime = lifetime();
    lifetime.observe(&request(1, "session/prompt"), false);
    lifetime.observe(&response(1), true);
    assert_eq!(expires_after(&lifetime).await, SHORT);
}

#[tokio::test(start_paused = true)]
async fn traffic_in_either_direction_renews_retention_without_promoting() {
    for from_client in [true, false] {
        let lifetime = lifetime();
        // Each gap stays inside the short period; together they exceed it.
        for _ in 0..5 {
            tokio::time::advance(SHORT / 2).await;
            lifetime.observe(&notification("session/cancel"), from_client);
        }
        assert_eq!(expires_after(&lifetime).await, SHORT, "{from_client}");
    }
}

#[tokio::test(start_paused = true)]
async fn read_only_traffic_renews_a_promoted_process_without_downgrading() {
    let lifetime = lifetime();
    lifetime.observe(&request(1, "session/new"), true);
    lifetime.observe(&response(1), false);
    // Each gap exceeds the short period.
    for id in 2..5 {
        tokio::time::advance(SHORT * 2).await;
        lifetime.observe(&request(id, "session/list"), true);
        lifetime.observe(&response(id), false);
    }
    assert_eq!(expires_after(&lifetime).await, LONG);
}

#[tokio::test(start_paused = true)]
async fn admitted_operation_suspends_expiration_until_released() {
    let lifetime = lifetime();
    let operation = lifetime.acquire().expect("live process admits work");
    let expiry = tokio::spawn({
        let lifetime = lifetime.clone();
        async move { lifetime.expired().await }
    });
    tokio::time::advance(LONG * 10).await;
    assert!(!expiry.is_finished());
    assert!(!lifetime.is_stopping());

    drop(operation);
    assert_eq!(expiry.await.unwrap(), SHORT);
}

#[tokio::test(start_paused = true)]
async fn pending_agent_request_suspends_expiration_until_the_client_responds() {
    let lifetime = lifetime();
    // Both peers may use the same request ID; an Agent response to the client's
    // request must not settle the Agent's own pending request.
    lifetime.observe(&request(7, "session/list"), true);
    lifetime.observe(&request(7, "fs/read_text_file"), false);
    lifetime.observe(&response(7), false);
    let expiry = tokio::spawn({
        let lifetime = lifetime.clone();
        async move { lifetime.expired().await }
    });
    tokio::time::advance(LONG * 10).await;
    assert!(!expiry.is_finished());

    lifetime.observe(&response(7), true);
    assert_eq!(expiry.await.unwrap(), SHORT);
}
