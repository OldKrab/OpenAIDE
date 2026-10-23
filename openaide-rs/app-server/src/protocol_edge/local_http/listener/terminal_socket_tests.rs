use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc;
use std::time::Instant;

use openaide_app_server_protocol::ids::ProjectId;
use serde_json::{json, Value};

use crate::protocol_edge::local_http::listener::handle_app_stream;
use crate::protocol_edge::local_http::LocalHttpAppHandler;
use crate::protocol_edge::stdio::ProtocolEdgeStdioDispatcher;
use crate::protocol_edge::SharedRpcGateway;
use crate::storage_runtime::StateRoot;
use crate::test_sync::WATCHDOG;

struct Server {
    address: SocketAddr,
    gateway: SharedRpcGateway,
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
            gateway.clone(),
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
            gateway,
            _state_dir: state_dir,
            _shutdown: shutdown,
        }
    }

    fn open_terminal(&self) -> (String, String) {
        let opened = self
            .gateway
            .terminals()
            .open(
                ProjectId::from("project".to_string()),
                Path::new("/"),
                80,
                24,
                "project",
            )
            .expect("shell starts");
        (opened.terminal.terminal_id, opened.attach.token)
    }

    fn connect(&self) -> Client {
        let mut stream = TcpStream::connect(self.address).expect("connect");
        stream
            .set_read_timeout(Some(crate::test_sync::WATCHDOG))
            .expect("read timeout");
        write!(
            stream,
            "GET /terminal HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        .expect("write upgrade");
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).expect("read upgrade response");
            response.push(byte[0]);
        }
        assert!(response.starts_with(b"HTTP/1.1 101"));
        Client { stream }
    }

    fn attach(&self, token: &str) -> Client {
        let mut client = self.connect();
        client.send_text(&json!({ "type": "hello", "token": token, "cols": 100, "rows": 30 }));
        match client.receive() {
            Received::Text(ready) => assert_eq!(ready["type"], "ready", "{ready}"),
            other => panic!("expected ready, got {other:?}"),
        }
        client
    }
}

#[derive(Debug, PartialEq)]
enum Received {
    Text(Value),
    Binary(Vec<u8>),
    Ping,
    Closed(u16),
}

struct Client {
    stream: TcpStream,
}

impl Client {
    fn send_text(&mut self, message: &Value) {
        self.send_frame(0x81, message.to_string().as_bytes());
    }

    fn send_frame(&mut self, first_byte: u8, payload: &[u8]) {
        let mask = [7_u8, 11, 13, 17];
        let mut frame = vec![first_byte, 0x80 | payload.len() as u8];
        assert!(payload.len() <= 125);
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(i, byte)| byte ^ mask[i % 4]),
        );
        self.stream.write_all(&frame).expect("write frame");
    }

    fn receive(&mut self) -> Received {
        let mut header = [0_u8; 2];
        self.stream.read_exact(&mut header).expect("frame header");
        let length = match header[1] & 0x7F {
            126 => {
                let mut bytes = [0_u8; 2];
                self.stream.read_exact(&mut bytes).unwrap();
                u16::from_be_bytes(bytes) as usize
            }
            127 => {
                let mut bytes = [0_u8; 8];
                self.stream.read_exact(&mut bytes).unwrap();
                u64::from_be_bytes(bytes) as usize
            }
            short => short as usize,
        };
        let mut payload = vec![0_u8; length];
        self.stream.read_exact(&mut payload).expect("frame payload");
        match header[0] & 0x0F {
            0x1 => Received::Text(serde_json::from_slice(&payload).unwrap()),
            0x2 => Received::Binary(payload),
            0x9 => Received::Ping,
            0x8 => Received::Closed(u16::from_be_bytes([payload[0], payload[1]])),
            opcode => panic!("unexpected opcode {opcode}"),
        }
    }

    /// Collects output until it contains `needle`.
    fn output_until(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + WATCHDOG;
        let mut seen = Vec::new();
        while !String::from_utf8_lossy(&seen).contains(needle) {
            assert!(Instant::now() < deadline, "no {needle:?} in output");
            match self.receive() {
                Received::Binary(bytes) => seen.extend(bytes),
                Received::Ping => {}
                Received::Text(message) if message["type"] == "ping" => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        String::from_utf8_lossy(&seen).into_owned()
    }
}

#[test]
fn streams_input_and_output_and_replays_to_a_reattaching_client() {
    let server = Server::start();
    let (terminal_id, token) = server.open_terminal();
    let mut first = server.attach(&token);

    first.send_frame(0x82, b"echo socket-$((6*7))\r");
    first.output_until("socket-42");
    drop(first);

    let renewed = server.gateway.terminals().attach(&terminal_id).unwrap();
    let mut second = server.attach(&renewed.token);
    second.output_until("socket-42");
    server.gateway.terminals().close_all();
}

#[test]
fn a_used_token_is_refused() {
    let server = Server::start();
    let (_, token) = server.open_terminal();
    let _first = server.attach(&token);

    let mut second = server.connect();
    second.send_text(&json!({ "type": "hello", "token": token, "cols": 80, "rows": 24 }));
    assert_eq!(second.receive(), Received::Closed(4401));
    server.gateway.terminals().close_all();
}

#[test]
fn closing_the_terminal_ends_its_socket() {
    let server = Server::start();
    let (terminal_id, token) = server.open_terminal();
    let mut client = server.attach(&token);

    server
        .gateway
        .terminals()
        .close(&terminal_id, true)
        .unwrap();
    let deadline = Instant::now() + WATCHDOG;
    loop {
        assert!(Instant::now() < deadline);
        match client.receive() {
            Received::Closed(code) => {
                assert_eq!(code, 1000);
                break;
            }
            Received::Binary(_) | Received::Ping | Received::Text(_) => {}
        }
    }
}

#[test]
fn a_malformed_message_is_a_protocol_violation() {
    let server = Server::start();
    let (_, token) = server.open_terminal();
    let mut client = server.attach(&token);

    client.send_text(&json!({ "type": "frame" }));
    loop {
        match client.receive() {
            Received::Closed(code) => {
                assert_eq!(code, 4400);
                break;
            }
            Received::Binary(_) | Received::Ping | Received::Text(_) => {}
        }
    }
    server.gateway.terminals().close_all();
}

#[test]
fn a_viewer_watching_a_busy_shell_is_still_pinged_and_its_pongs_keep_it() {
    let server = Server::start();
    let (_, token) = server.open_terminal();
    let mut client = server.attach(&token);

    // Output never pauses for a whole ping interval, which used to suppress the ping.
    client.send_frame(0x82, b"while :; do echo tick; sleep 0.05; done\r");
    let deadline = Instant::now() + WATCHDOG;
    let mut pings = 0;
    while pings < 3 {
        assert!(
            Instant::now() < deadline,
            "no ping during continuous output"
        );
        match client.receive() {
            Received::Text(message) if message["type"] == "ping" => {
                pings += 1;
                client.send_text(&json!({ "type": "pong" }));
            }
            Received::Binary(_) | Received::Ping => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    server.gateway.terminals().close_all();
}
