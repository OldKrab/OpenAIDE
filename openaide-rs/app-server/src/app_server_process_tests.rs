use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::app_server_client::runner::{
    AttachOrLaunchRequirements, AttachOrLaunchRunResult, AttachOrLaunchRunner,
};
use crate::app_server_client::StorageWriterState;
use crate::protocol_edge::stdio::ProtocolEdgeStdioDispatcher;
use crate::storage_runtime::{EndpointRecordStore, StateRoot};

use super::publish_local_http_probe_endpoint;

#[test]
fn published_local_http_endpoint_is_reused_by_attach_or_launch() {
    let state_dir = tempfile::TempDir::new().expect("state dir");
    let runtime_dir = tempfile::TempDir::new().expect("runtime dir");
    let state_root = StateRoot::resolve(state_dir.path()).expect("state root");
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root.clone());
    let probe_facts = dispatcher.shared_gateway().probe_facts();

    let _published = publish_local_http_probe_endpoint(
        dispatcher.shared_gateway(),
        &state_root,
        runtime_dir.path(),
    )
    .expect("publish endpoint");

    let runner = AttachOrLaunchRunner::new(
        EndpointRecordStore::new(runtime_dir.path()),
        runtime_dir.path().join("launch.lock"),
    );
    let result = runner
        .run_with_local_transports(
            state_root.fingerprint(),
            &AttachOrLaunchRequirements {
                required_protocol_version: probe_facts.protocol_version,
                required_app_version: probe_facts.app_version,
            },
            StorageWriterState::Available,
        )
        .expect("attach-or-launch");

    assert!(matches!(
        result,
        AttachOrLaunchRunResult::AttachExisting { .. }
    ));
}

#[test]
fn published_local_http_endpoint_serves_product_requests_with_connection_id() {
    let state_dir = tempfile::TempDir::new().expect("state dir");
    let runtime_dir = tempfile::TempDir::new().expect("runtime dir");
    let state_root = StateRoot::resolve(state_dir.path()).expect("state root");
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root.clone());
    let endpoint_records = EndpointRecordStore::new(runtime_dir.path());

    let _published = publish_local_http_probe_endpoint(
        dispatcher.shared_gateway(),
        &state_root,
        runtime_dir.path(),
    )
    .expect("publish endpoint");
    let record = endpoint_records
        .read(state_root.fingerprint())
        .unwrap()
        .expect("endpoint record");

    let response = post_json(
        &record.endpoints[0].address,
        &record.auth_token,
        Some("client-1"),
        &json!({
            "jsonrpc": "2.0",
            "id": "probe-product",
            "method": "client/probe",
            "params": {}
        })
        .to_string(),
    );

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    let body = response.split("\r\n\r\n").nth(1).expect("body");
    let messages: Value = serde_json::from_str(body).unwrap();
    assert_eq!(messages[0]["id"], "probe-product");
    assert_eq!(
        messages[0]["result"]["result"]["stateRootFingerprint"],
        state_root.fingerprint().as_str()
    );
}

#[test]
fn published_local_http_endpoint_accepts_next_request_while_one_connection_is_slow() {
    let state_dir = tempfile::TempDir::new().expect("state dir");
    let runtime_dir = tempfile::TempDir::new().expect("runtime dir");
    let state_root = StateRoot::resolve(state_dir.path()).expect("state root");
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root.clone());
    let endpoint_records = EndpointRecordStore::new(runtime_dir.path());

    let _published = publish_local_http_probe_endpoint(
        dispatcher.shared_gateway(),
        &state_root,
        runtime_dir.path(),
    )
    .expect("publish endpoint");
    let record = endpoint_records
        .read(state_root.fingerprint())
        .unwrap()
        .expect("endpoint record");

    let stalled = open_partial_post(&record.endpoints[0].address, &record.auth_token);
    let started = Instant::now();
    let response = post_json(
        &record.endpoints[0].address,
        &record.auth_token,
        None,
        &json!({
            "jsonrpc": "2.0",
            "id": "client_probe",
            "method": "client/probe",
            "params": {}
        })
        .to_string(),
    );
    drop(stalled);

    assert!(
        started.elapsed() < Duration::from_millis(500),
        "second request should not wait for the stalled connection"
    );
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
}

fn post_json(address: &str, token: &str, connection_id: Option<&str>, body: &str) -> String {
    let target = address.strip_prefix("http://").expect("http address");
    let (authority, path) = target
        .split_once('/')
        .map(|(authority, path)| (authority, format!("/{path}")))
        .unwrap_or((target, "/".to_string()));
    let connection_header = connection_id
        .map(|id| format!("X-OpenAIDE-Connection-Id: {id}\r\n"))
        .unwrap_or_default();
    let wire = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {token}\r\n{connection_header}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(authority).unwrap();
    stream.write_all(wire.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn open_partial_post(address: &str, token: &str) -> TcpStream {
    let target = address.strip_prefix("http://").expect("http address");
    let (authority, path) = target
        .split_once('/')
        .map(|(authority, path)| (authority, format!("/{path}")))
        .unwrap_or((target, "/".to_string()));
    let wire = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: 999\r\nConnection: close\r\n\r\n{{"
    );
    let mut stream = TcpStream::connect(authority).unwrap();
    stream.write_all(wire.as_bytes()).unwrap();
    stream
}
