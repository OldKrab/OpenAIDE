//! Drives the built App Server program the way an App Shell's SSH launcher does
//! on an SSH Computer (ADR-0069).

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const PROGRAM: &str = env!("CARGO_BIN_EXE_openaide-app-server");

fn keep(root: &Path, arguments: &[&str]) -> (bool, Value) {
    let output = Command::new(PROGRAM)
        .arg("keep")
        .args(arguments)
        .env("OPENAIDE_STORAGE_ROOT", root.join("state"))
        .env("OPENAIDE_RUNTIME_ROOT", root.join("run"))
        .env_remove("OPENAIDE_APP_SERVER_PROTOCOL")
        .env_remove("OPENAIDE_RUNTIME_PROTOCOL")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let report = serde_json::from_str(text.trim())
        .unwrap_or_else(|_| panic!("keep {arguments:?} printed {text:?}"));
    (output.status.success(), report)
}

/// Stops a keeper a failed assertion left behind.
struct StopKeeper<'a>(&'a Path);

impl Drop for StopKeeper<'_> {
    fn drop(&mut self) {
        let _ = keep(self.0, &["stop"]);
    }
}

/// A minimal App Shell: one HTTP request per call, as the launcher's client is.
struct Shell {
    address: SocketAddr,
    path: String,
    token: String,
    next: u32,
}

impl Shell {
    fn attach(report: &Value) -> Self {
        let url = report["endpointUrl"].as_str().unwrap();
        let rest = url.strip_prefix("http://").unwrap();
        let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let mut shell = Self {
            address: authority.parse().unwrap(),
            path: if path.is_empty() { "/" } else { path }.to_string(),
            token: report["authToken"].as_str().unwrap().to_string(),
            next: 0,
        };
        shell.call(
            "client/initialize",
            json!({
                "clientInstanceId": "desktop-1",
                "shell": { "kind": "desktop", "name": "Desktop", "version": "test" },
                "requestedSurface": { "kind": "home" },
            }),
        );
        shell
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = format!("shell-{}", self.next);
        let body =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        let mut stream = TcpStream::connect(self.address).unwrap();
        write!(
            stream,
            "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-OpenAIDE-Connection-Id: desktop-1\r\nX-OpenAIDE-Client-Instance-Id: desktop-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.path, self.address, self.token, body.len(), body
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (_, body) = response.split_once("\r\n\r\n").unwrap();
        let messages = match serde_json::from_str::<Value>(body).unwrap() {
            Value::Array(messages) => messages,
            message => vec![message],
        };
        let reply = messages
            .into_iter()
            .find(|message| message["id"] == id)
            .unwrap();
        assert!(reply.get("error").is_none(), "{method} failed: {reply}");
        reply["result"]["result"].clone()
    }
}

#[test]
fn start_brings_up_one_kept_app_server_that_stop_lets_go() {
    let root = tempfile::tempdir().unwrap();
    let _cleanup = StopKeeper(root.path());

    let (ok, first) = keep(root.path(), &["start"]);
    assert!(ok, "{first}");
    assert_eq!(first["outcome"], "started", "{first}");
    assert_eq!(first["keeper"], "new");
    assert_eq!(first["appVersion"], env!("CARGO_PKG_VERSION"));

    let (ok, second) = keep(root.path(), &["start"]);
    assert!(ok, "{second}");
    assert_eq!(second["outcome"], "attached", "{second}");
    assert_eq!(second["keeper"], "existing");
    assert_eq!(second["endpointUrl"], first["endpointUrl"]);

    let mut shell = Shell::attach(&second);
    let impact = shell.call("client/replacementImpact", json!({}));
    let kinds: Vec<&str> = impact["otherClients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|client| client["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["keeper"], "{impact}");

    let (ok, stopped) = keep(root.path(), &["stop"]);
    assert!(ok, "{stopped}");
    assert_eq!(stopped["outcome"], "stopped");
    let impact = shell.call("client/replacementImpact", json!({}));
    assert!(impact.get("otherClients").is_none(), "{impact}");

    let (ok, again) = keep(root.path(), &["stop"]);
    assert!(ok, "{again}");
    assert_eq!(again["outcome"], "not_running");
    shell.call("client/detach", json!({}));
}

#[test]
fn join_without_a_running_app_server_starts_nothing() {
    let root = tempfile::tempdir().unwrap();
    let _cleanup = StopKeeper(root.path());

    let (ok, report) = keep(root.path(), &["start", "--join"]);

    assert!(ok, "{report}");
    assert_eq!(
        report,
        json!({ "outcome": "failed", "error": "not_running" })
    );
}
