use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use serde_json::Value;

use super::{AppServerHold, HoldTarget, loopback_endpoint};

/// What the App Server received: the method, and the headers that identify the caller.
struct Received {
    method: String,
    params: Value,
    authorization: String,
    client_instance_id: String,
}

/// A listener that answers every request as the App Server does, or refuses while `refusing` is set.
struct FakeAppServer {
    target: HoldTarget,
    received: mpsc::Receiver<Received>,
    refusing: Arc<AtomicBool>,
}

impl FakeAppServer {
    fn start(auth_token: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake App Server");
        let address = listener.local_addr().expect("fake App Server address");
        let (sender, received) = mpsc::channel();
        let refusing = Arc::new(AtomicBool::new(false));
        let refuse = refusing.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                let mut headers = Vec::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    headers.push(line.trim_end().to_string());
                }
                let header = |name: &str| {
                    headers
                        .iter()
                        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
                        .unwrap_or_default()
                        .to_string()
                };
                let mut body = vec![0; header("Content-Length").parse().unwrap_or(0)];
                reader.read_exact(&mut body).expect("request body");
                let request: Value = serde_json::from_slice(&body).expect("request JSON");
                let reply = if refuse.load(Ordering::Acquire) {
                    serde_json::json!({ "jsonrpc": "2.0", "id": request["id"], "error": { "code": "clientNotInitialized" } })
                } else {
                    // The reply follows an event queued for the client, as on a real connection.
                    serde_json::json!([
                        { "jsonrpc": "2.0", "method": "app/event", "params": {} },
                        { "jsonrpc": "2.0", "id": request["id"], "result": { "result": {} } },
                    ])
                }
                .to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = sender.send(Received {
                    method: request["method"].as_str().unwrap_or_default().to_string(),
                    params: request["params"].clone(),
                    authorization: header("Authorization"),
                    client_instance_id: header("X-OpenAIDE-Client-Instance-Id"),
                });
            }
        });
        Self {
            target: HoldTarget {
                endpoint_url: format!("http://{address}/rpc"),
                auth_token: auth_token.to_string(),
            },
            received,
            refusing,
        }
    }

    /// Every request answered so far. A step returns only after its request was answered.
    fn methods(&self) -> Vec<String> {
        self.received
            .try_iter()
            .map(|request| request.method)
            .collect()
    }
}

#[test]
fn becomes_a_desktop_client_and_then_keeps_renewing_it() {
    let server = FakeAppServer::start("token-one");
    let hold = AppServerHold::new();

    hold.sync(Some(server.target.clone()));
    let initialize = server.received.try_recv().expect("initialize request");
    assert_eq!(initialize.method, "client/initialize");
    assert_eq!(initialize.authorization, "Bearer token-one");
    assert_eq!(initialize.params["shell"]["kind"], "desktop");
    assert_eq!(initialize.params["role"], "shellHold");
    assert_eq!(
        initialize.params["clientInstanceId"],
        initialize.client_instance_id
    );
    assert!(initialize.client_instance_id.starts_with("desktop-native-"));

    hold.sync(Some(server.target.clone()));
    hold.sync(Some(server.target.clone()));
    assert_eq!(server.methods(), ["client/heartbeat", "client/heartbeat"]);
}

#[test]
fn follows_a_replaced_app_server() {
    let first = FakeAppServer::start("token-one");
    let second = FakeAppServer::start("token-two");
    let hold = AppServerHold::new();

    hold.sync(Some(first.target.clone()));
    hold.sync(Some(second.target.clone()));
    hold.sync(Some(second.target.clone()));

    assert_eq!(first.methods(), ["client/initialize", "client/detach"]);
    assert_eq!(second.methods(), ["client/initialize", "client/heartbeat"]);
}

#[test]
fn starts_over_when_the_app_server_no_longer_knows_the_client() {
    let server = FakeAppServer::start("token-one");
    let hold = AppServerHold::new();
    hold.sync(Some(server.target.clone()));

    server.refusing.store(true, Ordering::Release);
    hold.sync(Some(server.target.clone()));
    server.refusing.store(false, Ordering::Release);
    hold.sync(Some(server.target.clone()));

    assert_eq!(
        server.methods(),
        ["client/initialize", "client/heartbeat", "client/initialize"]
    );
}

#[test]
fn lets_go_of_an_app_server_desktop_no_longer_owns() {
    let server = FakeAppServer::start("token-one");
    let hold = AppServerHold::new();
    hold.sync(Some(server.target.clone()));

    hold.sync(None);
    hold.sync(None);

    assert_eq!(server.methods(), ["client/initialize", "client/detach"]);
}

#[test]
fn ending_detaches_at_once_and_stays_ended() {
    let server = FakeAppServer::start("token-one");
    let hold = AppServerHold::new();
    hold.sync(Some(server.target.clone()));

    hold.end("quit");
    hold.sync(Some(server.target.clone()));
    hold.end("quit");

    assert_eq!(server.methods(), ["client/initialize", "client/detach"]);
}

#[test]
fn asks_again_after_a_refused_start() {
    let server = FakeAppServer::start("token-one");
    let hold = AppServerHold::new();

    server.refusing.store(true, Ordering::Release);
    hold.sync(Some(server.target.clone()));
    server.refusing.store(false, Ordering::Release);
    hold.sync(Some(server.target.clone()));
    hold.sync(Some(server.target.clone()));

    assert_eq!(
        server.methods(),
        ["client/initialize", "client/initialize", "client/heartbeat"]
    );
}

#[test]
fn sends_the_token_only_to_loopback() {
    assert_eq!(
        loopback_endpoint("http://127.0.0.1:43123/rpc")
            .map(|(address, path)| (address.port(), path)),
        Some((43123, "/rpc".to_string()))
    );
    assert!(loopback_endpoint("http://192.0.2.1:43123/rpc").is_none());
    assert!(loopback_endpoint("https://127.0.0.1:43123/rpc").is_none());
    assert!(loopback_endpoint("http://localhost:43123/rpc").is_none());
}
