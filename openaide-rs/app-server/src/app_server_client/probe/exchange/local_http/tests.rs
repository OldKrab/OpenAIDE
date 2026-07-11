use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::app_server_client::probe::EndpointProbeEndpoint;
use crate::storage_runtime::RuntimeEndpoint;

#[test]
fn posts_probe_request_with_auth_and_returns_json_response() {
    let server = TestServer::spawn(json_response(
        r#"{"jsonrpc":"2.0","id":"client_probe","result":{}}"#,
    ));
    let mut exchange = LocalHttpProbeExchange::with_timeout(Duration::from_secs(1));
    let runtime_endpoint = runtime_endpoint(&server.address);

    let response = exchange
        .exchange(
            ClientProbeExchangeEndpoint {
                endpoint: endpoint(&runtime_endpoint),
            },
            json!({"jsonrpc": "2.0", "id": "client_probe"}),
        )
        .unwrap();

    assert_eq!(
        response,
        ClientProbeExchangeResponse::Json(json!({
            "jsonrpc": "2.0",
            "id": "client_probe",
            "result": {}
        }))
    );
    let request = server.request.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(request.starts_with("POST /probe HTTP/1.1\r\n"));
    assert!(request.contains("Authorization: Bearer token\r\n"));
    assert!(request.contains("Content-Type: application/json\r\n"));
    assert!(request.ends_with(r#"{"id":"client_probe","jsonrpc":"2.0"}"#));
}

#[test]
fn complete_content_length_response_does_not_wait_for_eof() {
    let server = TestServer::spawn_keep_open(json_response(
        r#"{"jsonrpc":"2.0","id":"client_probe","result":{}}"#,
    ));
    let mut exchange = LocalHttpProbeExchange::with_timeout(Duration::from_secs(1));
    let runtime_endpoint = runtime_endpoint(&server.address);

    assert!(matches!(
        exchange
            .exchange(
                ClientProbeExchangeEndpoint {
                    endpoint: endpoint(&runtime_endpoint),
                },
                json!({}),
            )
            .unwrap(),
        ClientProbeExchangeResponse::Json(_)
    ));
}

#[test]
fn auth_statuses_map_to_auth_failed() {
    for status in [401, 403] {
        let server = TestServer::spawn(format!(
            "HTTP/1.1 {status} Nope\r\nContent-Length: 0\r\n\r\n"
        ));
        let mut exchange = LocalHttpProbeExchange::with_timeout(Duration::from_secs(1));
        let runtime_endpoint = runtime_endpoint(&server.address);

        assert_eq!(
            exchange
                .exchange(
                    ClientProbeExchangeEndpoint {
                        endpoint: endpoint(&runtime_endpoint),
                    },
                    json!({}),
                )
                .unwrap(),
            ClientProbeExchangeResponse::AuthFailed
        );
    }
}

#[test]
fn non_loopback_address_fails_before_sending_token() {
    let mut exchange = LocalHttpProbeExchange::with_timeout(Duration::from_millis(100));
    let runtime_endpoint = runtime_endpoint("http://192.0.2.1:1234/probe");

    let error = exchange
        .exchange(
            ClientProbeExchangeEndpoint {
                endpoint: endpoint(&runtime_endpoint),
            },
            json!({}),
        )
        .unwrap_err();

    assert!(error.message.contains("loopback"));
}

#[test]
fn accepted_then_stalled_response_maps_to_unreachable() {
    let server = TestServer::spawn_stalled();
    let mut exchange = LocalHttpProbeExchange::with_timeout(Duration::from_millis(100));
    let runtime_endpoint = runtime_endpoint(&server.address);

    assert_eq!(
        exchange
            .exchange(
                ClientProbeExchangeEndpoint {
                    endpoint: endpoint(&runtime_endpoint),
                },
                json!({}),
            )
            .unwrap(),
        ClientProbeExchangeResponse::Unreachable
    );
}

#[test]
fn connection_failure_maps_to_unreachable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/probe", listener.local_addr().unwrap());
    drop(listener);
    let mut exchange = LocalHttpProbeExchange::with_timeout(Duration::from_millis(100));
    let runtime_endpoint = runtime_endpoint(&address);

    assert_eq!(
        exchange
            .exchange(
                ClientProbeExchangeEndpoint {
                    endpoint: endpoint(&runtime_endpoint),
                },
                json!({}),
            )
            .unwrap(),
        ClientProbeExchangeResponse::Unreachable
    );
}

#[test]
fn malformed_address_response_or_json_fails_probe() {
    let mut exchange = LocalHttpProbeExchange::with_timeout(Duration::from_secs(1));
    let invalid_scheme = runtime_endpoint("https://127.0.0.1:1/probe");
    assert!(exchange
        .exchange(
            ClientProbeExchangeEndpoint {
                endpoint: endpoint(&invalid_scheme),
            },
            json!({}),
        )
        .is_err());

    let malformed_response = TestServer::spawn("not-http\r\n\r\n{}".to_string());
    let malformed_endpoint = runtime_endpoint(&malformed_response.address);
    assert!(exchange
        .exchange(
            ClientProbeExchangeEndpoint {
                endpoint: endpoint(&malformed_endpoint),
            },
            json!({}),
        )
        .is_err());

    let invalid_json =
        TestServer::spawn("HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n{not-json".to_string());
    let invalid_json_endpoint = runtime_endpoint(&invalid_json.address);
    assert!(exchange
        .exchange(
            ClientProbeExchangeEndpoint {
                endpoint: endpoint(&invalid_json_endpoint),
            },
            json!({}),
        )
        .is_err());
}

#[test]
fn supports_only_local_http() {
    let exchange = LocalHttpProbeExchange::default();

    assert!(exchange.supports_transport(TransportKind::LocalHttp));
    assert!(!exchange.supports_transport(TransportKind::Stdio));
}

struct TestServer {
    address: String,
    request: mpsc::Receiver<String>,
}

impl TestServer {
    fn spawn(response: String) -> Self {
        Self::spawn_with(response, ServerMode::CloseAfterWrite)
    }

    fn spawn_keep_open(response: String) -> Self {
        Self::spawn_with(response, ServerMode::KeepOpenAfterWrite)
    }

    fn spawn_stalled() -> Self {
        Self::spawn_with(String::new(), ServerMode::Stall)
    }

    fn spawn_with(response: String, mode: ServerMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/probe", listener.local_addr().unwrap());
        let (request_tx, request_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).unwrap();
            request_tx
                .send(String::from_utf8_lossy(&buffer[..read]).into_owned())
                .unwrap();
            match mode {
                ServerMode::CloseAfterWrite => {
                    stream.write_all(response.as_bytes()).unwrap();
                }
                ServerMode::KeepOpenAfterWrite => {
                    stream.write_all(response.as_bytes()).unwrap();
                    std::thread::sleep(Duration::from_millis(250));
                }
                ServerMode::Stall => {
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        });
        Self {
            address,
            request: request_rx,
        }
    }
}

enum ServerMode {
    CloseAfterWrite,
    KeepOpenAfterWrite,
    Stall,
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}

fn runtime_endpoint(address: &str) -> RuntimeEndpoint {
    RuntimeEndpoint {
        transport: TransportKind::LocalHttp,
        address: address.to_string(),
    }
}

fn endpoint(endpoint: &RuntimeEndpoint) -> EndpointProbeEndpoint<'_> {
    EndpointProbeEndpoint {
        endpoint,
        auth_token: "token",
    }
}
