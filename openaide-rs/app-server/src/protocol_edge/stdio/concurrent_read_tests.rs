//! The stdin loop runs read-only requests beside itself so one slow read cannot hold up the
//! lines behind it (see `concurrent_reads`).

use std::sync::mpsc;

use openaide_app_server_protocol::methods::{CLIENT_HEARTBEAT, FILE_VIEWER_CHANGES};
use serde_json::json;

use super::file_viewer_tests::{init_request, response};
use super::ProtocolEdgeStdioDispatcher;
use crate::storage_runtime::StateRoot;

fn line(id: &str, method: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": {} }).to_string()
}

fn dispatcher() -> (tempfile::TempDir, ProtocolEdgeStdioDispatcher) {
    let temp = tempfile::TempDir::new().unwrap();
    let dispatcher =
        ProtocolEdgeStdioDispatcher::new_for_test(StateRoot::resolve(temp.path()).unwrap());
    (temp, dispatcher)
}

#[test]
fn only_a_single_read_only_request_is_claimed() {
    let (_temp, mut dispatcher) = dispatcher();

    assert!(dispatcher
        .claim_concurrent_read(&line("read", FILE_VIEWER_CHANGES))
        .is_some());
    assert!(dispatcher
        .claim_concurrent_read(&line("beat", CLIENT_HEARTBEAT))
        .is_none());
    let notification = json!({ "jsonrpc": "2.0", "method": FILE_VIEWER_CHANGES }).to_string();
    assert!(dispatcher.claim_concurrent_read(&notification).is_none());
    let batch = format!("[{}]", line("read", FILE_VIEWER_CHANGES));
    assert!(dispatcher.claim_concurrent_read(&batch).is_none());
    assert!(dispatcher.claim_concurrent_read("not json").is_none());
}

#[test]
fn a_claimed_read_answers_on_the_outbound_sequence_when_it_ends() {
    let (_temp, mut dispatcher) = dispatcher();
    dispatcher.handle_line(&init_request("init", "client-1"));
    let read = dispatcher
        .claim_concurrent_read(&line("read", FILE_VIEWER_CHANGES))
        .expect("claimed");
    assert_eq!(read.method(), FILE_VIEWER_CHANGES);
    let outbound = dispatcher.outbound();

    let (done, finished) = mpsc::channel();
    read.start(move || done.send(()).unwrap());
    crate::test_sync::wait_for("the read ends", || finished.try_recv().ok());

    let lines = outbound.flush(|lines| lines);
    let reply = lines
        .iter()
        .map(|line| response(line))
        .find(|message| message["id"] == "read")
        .expect("the read's reply joined the outbound sequence");
    assert!(reply.get("error").is_some() || reply.get("result").is_some());
}
