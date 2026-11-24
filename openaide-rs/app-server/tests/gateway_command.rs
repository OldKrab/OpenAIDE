//! Drives the built App Server program the way the Web App's server runs its
//! gateway (ADR-0082): JSON lines on stdin and stdout, until stdin closes.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

const PROGRAM: &str = env!("CARGO_BIN_EXE_openaide-app-server");

fn gateway(root: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(PROGRAM);
    command
        .arg("gateway")
        .args(arguments)
        .env("OPENAIDE_STORAGE_ROOT", root.join("state"))
        .env("OPENAIDE_RUNTIME_ROOT", root.join("run"))
        .env_remove("OPENAIDE_APP_SERVER_PROTOCOL")
        .env_remove("OPENAIDE_RUNTIME_PROTOCOL");
    command
}

#[test]
fn the_gateway_answers_on_stdout_and_exits_when_stdin_closes() {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("state").join("web-app");
    let mut child = gateway(root.path(), &["--state", folder.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut next =
        || serde_json::from_str::<serde_json::Value>(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(next(), serde_json::json!({ "event": "ready" }));

    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(b"{\"id\":1,\"method\":\"computers\"}\n")
        .unwrap();
    assert_eq!(
        next(),
        serde_json::json!({ "id": 1, "result": { "computers": [] } })
    );

    drop(stdin);
    assert!(child.wait().unwrap().success());
    // The key is the Web App's own, kept where only its owner reads it.
    assert!(folder.join("device.key").is_file());
}

#[test]
fn the_gateway_needs_its_folder() {
    let root = tempfile::tempdir().unwrap();
    let output = gateway(root.path(), &[])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("gateway --state <folder>"));
}
