use std::io::{Error, ErrorKind};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;

use super::*;

#[test]
fn expected_socket_interruptions_are_classified_as_transient() {
    for kind in [
        ErrorKind::Interrupted,
        ErrorKind::TimedOut,
        ErrorKind::WouldBlock,
        ErrorKind::BrokenPipe,
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
    ] {
        let error = LocalHttpProbeListenerError::Io(Error::from(kind));
        assert!(error.is_transient_io(), "{kind:?} should be transient");
    }

    let error = LocalHttpProbeListenerError::Io(Error::from(ErrorKind::InvalidData));
    assert!(!error.is_transient_io());
    assert!(!LocalHttpProbeListenerError::NonLoopbackPeer.is_transient_io());
}

#[test]
fn contextual_socket_errors_include_request_diagnostics() {
    let error = LocalHttpProbeListenerError::Io(Error::from(ErrorKind::WouldBlock))
        .with_request_context("write_response", Some("POST"), Some("client-1"), Some(200));

    let fields = error.diagnostic_fields();

    assert_eq!(fields["transient"], true);
    assert_eq!(fields["ioKind"], "WouldBlock");
    assert_eq!(fields["phase"], "write_response");
    assert_eq!(fields["method"], "POST");
    assert_eq!(fields["connectionId"], "client-1");
    assert_eq!(fields["status"], 200);
}

#[test]
fn handles_post_and_delegates_authorization_and_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (result_tx, result_rx) = mpsc::channel();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        handle_stream(&mut stream, |request| {
            result_tx
                .send((request.authorization, request.connection_id, request.body))
                .unwrap();
            LocalHttpResponse {
                status: 200,
                body: "{\"ok\":true}".to_string(),
            }
        })
        .unwrap();
    });

    let response = send(
        addr,
        "POST /probe HTTP/1.1\r\nAuthorization: Bearer token\r\nX-OpenAIDE-Connection-Id: client-1\r\nContent-Length: 14\r\n\r\n{\"probe\":true}",
    );
    server.join().unwrap();

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("Content-Type: application/json\r\n"));
    assert!(response.contains("Content-Length: 11\r\n"));
    assert!(response.ends_with("{\"ok\":true}"));
    assert_eq!(
        result_rx.recv().unwrap(),
        (
            Some("Bearer token".to_string()),
            Some("client-1".to_string()),
            "{\"probe\":true}".to_string()
        )
    );
}

#[test]
fn streams_event_data_without_waiting_for_a_json_rpc_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        handle_stream_with_push(
            &mut stream,
            |_request| panic!("event stream must not enter JSON-RPC handling"),
            |stream, request| {
                assert_eq!(request.authorization.as_deref(), Some("Bearer token"));
                assert_eq!(request.connection_id.as_deref(), Some("client-1"));
                write_event_stream_headers(stream)?;
                write_event_stream_data(
                    stream,
                    r#"[{"jsonrpc":"2.0","method":"app/event","params":{"cursor":"2"}}]"#,
                )
            },
        )
        .unwrap();
    });

    let response = send(
        addr,
        "POST /probe HTTP/1.1\r\nAuthorization: Bearer token\r\nX-OpenAIDE-Connection-Id: client-1\r\nAccept: text/event-stream\r\nContent-Length: 0\r\n\r\n",
    );
    server.join().unwrap();

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("Content-Type: text/event-stream\r\n"));
    assert!(response.contains("data: [{\"jsonrpc\":\"2.0\",\"method\":\"app/event\""));
}

#[test]
fn malformed_request_returns_400_without_delegating() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (called_tx, called_rx) = mpsc::channel();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(handle_stream(&mut stream, |_request| {
            called_tx.send(()).unwrap();
            LocalHttpResponse {
                status: 200,
                body: "{}".to_string(),
            }
        })
        .is_err());
    });

    let response = send(addr, "POST /probe HTTP/1.1\r\n\r\n{}");
    server.join().unwrap();

    assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    assert!(called_rx.try_recv().is_err());
}

#[test]
fn oversized_body_returns_400_without_delegating() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (called_tx, called_rx) = mpsc::channel();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(handle_stream(&mut stream, |_request| {
            called_tx.send(()).unwrap();
            LocalHttpResponse {
                status: 200,
                body: "{}".to_string(),
            }
        })
        .is_err());
    });

    let response = send(
        addr,
        "POST /probe HTTP/1.1\r\nContent-Length: 10485761\r\n\r\n{}",
    );
    server.join().unwrap();

    assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    assert!(called_rx.try_recv().is_err());
}

#[test]
fn non_post_returns_405_without_delegating() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (called_tx, called_rx) = mpsc::channel();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        handle_stream(&mut stream, |_request| {
            called_tx.send(()).unwrap();
            LocalHttpResponse {
                status: 200,
                body: "{}".to_string(),
            }
        })
        .unwrap();
    });

    let response = send(addr, "GET /probe HTTP/1.1\r\nContent-Length: 0\r\n\r\n");
    server.join().unwrap();

    assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
    assert!(called_rx.try_recv().is_err());
}

#[test]
fn options_preflight_returns_cors_headers_without_delegating() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (called_tx, called_rx) = mpsc::channel();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        handle_stream(&mut stream, |_request| {
            called_tx.send(()).unwrap();
            LocalHttpResponse {
                status: 200,
                body: "{}".to_string(),
            }
        })
        .unwrap();
    });

    let response = send(
        addr,
        "OPTIONS /probe HTTP/1.1\r\nOrigin: http://localhost\r\nAccess-Control-Request-Headers: authorization,x-openaide-connection-id\r\n\r\n",
    );
    server.join().unwrap();

    assert!(response.starts_with("HTTP/1.1 204 No Content\r\n"));
    assert!(response.contains("Access-Control-Allow-Origin: *\r\n"));
    assert!(response.contains("Access-Control-Allow-Methods: POST, OPTIONS\r\n"));
    assert!(response.contains("Authorization, Content-Type, X-OpenAIDE-Connection-Id"));
    assert!(called_rx.try_recv().is_err());
}

fn send(addr: std::net::SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}
