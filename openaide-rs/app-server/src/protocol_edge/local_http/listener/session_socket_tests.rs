use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::protocol_edge::local_http::listener::handle_app_stream;
use crate::protocol_edge::local_http::LocalHttpAppHandler;
use crate::protocol_edge::stdio::ProtocolEdgeStdioDispatcher;
use crate::storage_runtime::StateRoot;

const CONNECTION_ID: &str = "socket-client-1";

/// A real App Server handler behind a loopback listener, one thread per
/// accepted connection like the production listener.
struct Server {
    address: SocketAddr,
    _state_dir: tempfile::TempDir,
    _shutdown: mpsc::Receiver<()>,
}

impl Server {
    fn start() -> Self {
        let state_dir = tempfile::TempDir::new().expect("state dir");
        let state_root = StateRoot::resolve(state_dir.path()).expect("state root");
        let gateway = ProtocolEdgeStdioDispatcher::new_for_test(state_root).shared_gateway();
        let (shutdown_sender, shutdown) = mpsc::channel();
        let handler = LocalHttpAppHandler::new(
            gateway,
            "token",
            "server-1",
            "replacement-token",
            shutdown_sender,
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().expect("listener address");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let handler = handler.clone();
                std::thread::spawn(move || {
                    let _ = handle_app_stream(&mut stream, &handler);
                });
            }
        });
        Self {
            address,
            _state_dir: state_dir,
            _shutdown: shutdown,
        }
    }

    fn connect(&self, authorization: Option<&str>) -> Client {
        let mut stream = TcpStream::connect(self.address).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let authorization = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "GET /socket HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{authorization}\r\n"
        )
        .expect("write upgrade");
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).expect("read upgrade response");
            response.push(byte[0]);
        }
        let response = String::from_utf8(response).expect("upgrade response is text");
        assert!(response.starts_with("HTTP/1.1 101"), "{response}");
        assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
        Client { stream }
    }

    /// Opens a link and returns it with the server's `ready` message.
    fn open(&self, hello: Value) -> (Client, Value) {
        let mut client = self.connect(Some("Bearer token"));
        client.send(&hello);
        let ready = client.receive();
        assert_eq!(ready["type"], "ready", "{ready}");
        (client, ready)
    }
}

#[derive(Debug, PartialEq)]
enum Received {
    Message(Value),
    Closed(u16),
}

struct Client {
    stream: TcpStream,
}

impl Client {
    fn send(&mut self, message: &Value) {
        self.send_frame(0x81, message.to_string().as_bytes());
    }

    fn send_frame(&mut self, first_byte: u8, payload: &[u8]) {
        let mask = [7_u8, 11, 13, 17];
        let mut frame = vec![first_byte];
        if payload.len() <= 125 {
            frame.push(0x80 | payload.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        self.stream.write_all(&frame).expect("write frame");
    }

    fn receive_any(&mut self) -> Received {
        loop {
            let mut header = [0_u8; 2];
            self.stream.read_exact(&mut header).expect("frame header");
            assert_eq!(header[1] & 0x80, 0, "server frames are unmasked");
            let length = match header[1] {
                126 => {
                    let mut bytes = [0_u8; 2];
                    self.stream.read_exact(&mut bytes).expect("length");
                    usize::from(u16::from_be_bytes(bytes))
                }
                127 => {
                    let mut bytes = [0_u8; 8];
                    self.stream.read_exact(&mut bytes).expect("length");
                    u64::from_be_bytes(bytes) as usize
                }
                short => usize::from(short),
            };
            let mut payload = vec![0_u8; length];
            self.stream.read_exact(&mut payload).expect("payload");
            match header[0] & 0x0F {
                0x1 => {
                    return Received::Message(
                        serde_json::from_slice(&payload).expect("JSON message"),
                    )
                }
                0x8 => return Received::Closed(u16::from_be_bytes([payload[0], payload[1]])),
                0xA => continue,
                opcode => panic!("unexpected opcode {opcode}"),
            }
        }
    }

    fn receive(&mut self) -> Value {
        match self.receive_any() {
            Received::Message(message) => message,
            Received::Closed(code) => panic!("socket closed with {code}"),
        }
    }

    /// Next message of one type; acks and frames may interleave freely.
    fn receive_type(&mut self, kind: &str) -> Value {
        loop {
            let message = self.receive();
            if message["type"] == kind {
                return message;
            }
        }
    }

    fn initialize(&mut self) -> Value {
        self.send(&frame(
            1,
            "initialize",
            "client/initialize",
            json!({
                "clientInstanceId": "socket-host-1",
                "shell": { "kind": "vscodeExtension", "name": "OpenAIDE" },
                "requestedSurface": { "kind": "home" },
                "workspaceRoots": []
            }),
        ));
        self.receive_type("frame")
    }
}

fn hello(session: Option<(&str, u64)>) -> Value {
    json!({
        "type": "hello",
        "transportVersion": 1,
        "connectionId": CONNECTION_ID,
        "sessionId": session.map(|(session_id, _)| session_id),
        "receivedThrough": session.map_or(0, |(_, received_through)| received_through),
    })
}

fn frame(sequence: u64, id: &str, method: &str, params: Value) -> Value {
    json!({
        "type": "frame",
        "sequence": sequence,
        "message": {
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
            "meta": {},
        }
    })
}

#[test]
fn a_socket_carries_requests_and_their_replies_on_one_connection() {
    let server = Server::start();
    let (mut client, ready) = server.open(hello(None));
    assert_eq!(ready["serverId"], "server-1");
    assert_eq!(ready["receivedThrough"], 0);

    let initialized = client.initialize();
    assert_eq!(initialized["sequence"], 1);
    assert_eq!(initialized["message"]["id"], "initialize");
    assert!(
        initialized["message"]["result"].is_object(),
        "{initialized}"
    );

    client.send(&frame(2, "beat", "client/heartbeat", json!({})));
    assert_eq!(client.receive_type("ack")["through"], 2);
    let reply = client.receive_type("frame");
    assert_eq!(reply["sequence"], 2);
    assert_eq!(reply["message"]["id"], "beat");
}

#[test]
fn a_direct_client_authenticates_in_the_hello() {
    let server = Server::start();
    let mut client = server.connect(None);
    let mut with_token = hello(None);
    with_token["authToken"] = json!("token");

    client.send(&with_token);

    assert_eq!(client.receive()["type"], "ready");
}

#[test]
fn a_resumed_socket_replays_unacknowledged_frames_and_skips_duplicates() {
    let server = Server::start();
    let (mut first, ready) = server.open(hello(None));
    let session_id = ready["sessionId"].as_str().unwrap().to_string();
    first.initialize();
    drop(first);

    // The reply was written but never acknowledged, so it is replayed.
    let (mut resumed, ready) = server.open(hello(Some((&session_id, 0))));
    assert_eq!(ready["sessionId"], session_id.as_str());
    assert_eq!(ready["receivedThrough"], 1);
    let replayed = resumed.receive_type("frame");
    assert_eq!(replayed["sequence"], 1);
    assert_eq!(replayed["message"]["id"], "initialize");

    // Resending an accepted frame is acknowledged without running it again:
    // the next frame the server produces answers the new request.
    resumed.send(&frame(1, "initialize", "client/heartbeat", json!({})));
    assert_eq!(resumed.receive_type("ack")["through"], 1);
    resumed.send(&json!({ "type": "ack", "through": 1 }));
    resumed.send(&frame(2, "beat", "client/heartbeat", json!({})));
    let reply = resumed.receive_type("frame");
    assert_eq!(reply["sequence"], 2);
    assert_eq!(reply["message"]["id"], "beat");
}

#[test]
fn a_newer_socket_supersedes_the_one_it_replaces() {
    let server = Server::start();
    let (mut first, ready) = server.open(hello(None));
    let session_id = ready["sessionId"].as_str().unwrap().to_string();

    let (_second, _) = server.open(hello(Some((&session_id, 0))));

    assert_eq!(first.receive_any(), Received::Closed(4000));
}

#[test]
fn definite_rejections_are_reported_as_close_codes() {
    let server = Server::start();

    let mut unknown_session = server.connect(Some("Bearer token"));
    unknown_session.send(&hello(Some(("missing-session", 0))));
    assert_eq!(unknown_session.receive_any(), Received::Closed(4410));

    let mut wrong_token = server.connect(Some("Bearer other"));
    wrong_token.send(&hello(None));
    assert_eq!(wrong_token.receive_any(), Received::Closed(4403));

    let mut no_token = server.connect(None);
    no_token.send(&hello(None));
    assert_eq!(no_token.receive_any(), Received::Closed(4401));

    let mut not_a_hello = server.connect(Some("Bearer token"));
    not_a_hello.send(&json!({ "type": "ack", "through": 0 }));
    assert_eq!(not_a_hello.receive_any(), Received::Closed(4400));

    let (mut gap, _) = server.open(hello(None));
    gap.send(&frame(5, "beat", "client/heartbeat", json!({})));
    assert_eq!(gap.receive_any(), Received::Closed(4400));

    let (mut ahead, _) = server.open(hello(None));
    ahead.send(&json!({ "type": "ack", "through": 9 }));
    assert_eq!(ahead.receive_any(), Received::Closed(4400));
}

#[test]
fn pings_are_answered_at_both_levels() {
    let server = Server::start();
    let (mut client, _) = server.open(hello(None));

    client.send(&json!({ "type": "ping" }));
    assert_eq!(client.receive()["type"], "pong");

    // A protocol-level ping gets a protocol-level pong, which `receive` skips;
    // the socket staying usable afterwards is what matters.
    client.send_frame(0x89, b"probe");
    client.send(&json!({ "type": "ping" }));
    assert_eq!(client.receive()["type"], "pong");
}
