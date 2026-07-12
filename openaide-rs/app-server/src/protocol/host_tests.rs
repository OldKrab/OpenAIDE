use std::time::Duration;

use serde_json::json;

use super::*;

#[test]
fn request_serializes_to_host_and_accepts_response() {
    let (bridge, requests) = HostBridge::channel_with_timeout(Duration::from_secs(1));

    let bridge_for_request = bridge.clone();
    let pending = std::thread::spawn(move || {
        bridge_for_request.request("fs/read_text_file", Some(json!({ "path": "/tmp/a.rs" })))
    });

    let outbound = requests
        .recv_timeout(Duration::from_secs(1))
        .expect("host request should be emitted");
    assert_eq!(outbound.jsonrpc, "2.0");
    assert_eq!(outbound.method, "fs/read_text_file");
    assert_eq!(outbound.params, Some(json!({ "path": "/tmp/a.rs" })));

    let handled = bridge.try_handle_response(&json!({
        "jsonrpc": "2.0",
        "id": outbound.id,
        "result": { "content": "ok" }
    }));

    assert!(handled);
    assert_eq!(pending.join().unwrap().unwrap(), json!({ "content": "ok" }));
}

#[test]
fn disabled_bridge_rejects_requests() {
    let error = HostBridge::disabled()
        .request("fs/read_text_file", None)
        .expect_err("disabled host bridge should reject requests");

    assert!(matches!(error, RuntimeError::CapabilityMissing(_)));
}

#[test]
fn request_until_can_wait_without_default_timeout_and_cancel() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let (bridge, requests) = HostBridge::channel_with_timeout(Duration::from_millis(1));
    let cancelled = Arc::new(AtomicBool::new(false));
    let request_cancelled = cancelled.clone();

    let pending = std::thread::spawn(move || {
        bridge.request_until("terminal/wait_for_exit", None, None, || {
            request_cancelled.load(Ordering::SeqCst)
        })
    });

    requests
        .recv_timeout(Duration::from_secs(1))
        .expect("host request should be emitted");
    cancelled.store(true, Ordering::SeqCst);

    let error = pending
        .join()
        .expect("host request thread")
        .expect_err("request should be cancelled");
    assert!(matches!(error, RuntimeError::NotReady(_)));
}
