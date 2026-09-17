use std::io::{Read, Write};
use std::net::TcpListener;

use super::*;

fn connection_for_port(port: u16) -> LocalHttpConnection {
    LocalHttpConnection {
        kind: "localHttp".to_string(),
        endpoint_url: format!("http://127.0.0.1:{port}/probe"),
        auth_token: "a".repeat(64),
    }
}

fn unused_loopback_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    listener
        .local_addr()
        .expect("loopback address")
        .port()
}

#[test]
fn any_http_response_proves_the_app_server_is_listening() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let port = listener.local_addr().expect("loopback address").port();
    let responder = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            // An unauthenticated 401 still proves a live App Server owns the endpoint.
            let _ = stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });

    let alive = tauri::async_runtime::block_on(app_server_endpoint_reachable(&connection_for_port(
        port,
    )));
    responder.join().expect("responder thread");

    assert!(alive);
}

#[test]
fn a_closed_endpoint_reports_the_app_server_as_gone() {
    let port = unused_loopback_port();

    let alive = tauri::async_runtime::block_on(app_server_endpoint_reachable(&connection_for_port(
        port,
    )));

    assert!(!alive, "a refused loopback connection means no process is listening");
}

#[test]
fn an_unparsable_endpoint_reports_the_app_server_as_gone() {
    let mut connection = connection_for_port(unused_loopback_port());
    connection.endpoint_url = "not a url".to_string();

    assert!(!tauri::async_runtime::block_on(app_server_endpoint_reachable(
        &connection
    )));
}
