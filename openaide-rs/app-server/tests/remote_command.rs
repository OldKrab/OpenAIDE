//! Drives the built App Server program the way a person does in a terminal.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::sync::mpsc;

#[allow(dead_code)]
#[path = "../src/test_sync.rs"]
mod test_sync;

const PROGRAM: &str = env!("CARGO_BIN_EXE_openaide-app-server");

fn command(root: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(PROGRAM);
    command
        .arg("remote")
        .args(arguments)
        .env("OPENAIDE_STORAGE_ROOT", root.join("state"))
        .env("OPENAIDE_RUNTIME_ROOT", root.join("run"))
        .env_remove("OPENAIDE_APP_SERVER_PROTOCOL")
        .env_remove("OPENAIDE_RUNTIME_PROTOCOL")
        .stdin(Stdio::null());
    command
}

fn output(root: &Path, arguments: &[&str]) -> (bool, String) {
    let Output {
        status,
        stdout,
        stderr,
    } = command(root, arguments).output().unwrap();
    (
        status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        ),
    )
}

/// The long-running command with its output read line by line.
struct Running {
    child: Child,
    lines: mpsc::Receiver<String>,
}

impl Running {
    fn start(root: &Path, arguments: &[&str]) -> Self {
        let mut child = command(root, arguments)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout: ChildStdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    fn line_with(&self, part: &str) -> String {
        loop {
            let line = self
                .lines
                .recv_timeout(test_sync::WATCHDOG)
                .unwrap_or_else(|_| panic!("no line with {part:?}"));
            if line.contains(part) {
                return line;
            }
        }
    }

    fn interrupt(&self) {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(self.child.id() as i32),
            nix::sys::signal::Signal::SIGINT,
        )
        .unwrap();
    }

    fn stopped(mut self) -> bool {
        self.line_with("Stopped.");
        self.child.wait().unwrap().success()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn the_command_keeps_the_app_server_up_for_other_commands_until_it_is_stopped() {
    let root = tempfile::tempdir().unwrap();

    let (ok, text) = output(root.path(), &["devices"]);
    assert!(!ok && text.contains("is not running"), "{text}");

    let running = Running::start(root.path(), &[]);
    running.line_with("Press Ctrl+C to stop.");

    let (ok, text) = output(root.path(), &["devices"]);
    assert!(ok && text.contains("No devices are paired"), "{text}");
    let (ok, text) = output(root.path(), &["remove", "1", "--yes"]);
    assert!(!ok && text.contains("no device number 1"), "{text}");
    let (ok, text) = output(root.path(), &["devcies"]);
    assert!(!ok && text.contains("Unknown command"), "{text}");

    running.interrupt();
    assert!(running.stopped());
}

/// Pairs a real device over the network, so it runs on request:
/// `cargo test -p openaide-app-server --test remote_command -- --ignored`.
#[test]
#[ignore = "reaches the public relay network"]
fn a_device_pairs_with_the_code_from_the_terminal_and_is_removed_there() {
    use openaide_remote_client::{bind, new_device_key, redeem, InviteCode, Logger, RemoteClient};

    let root = tempfile::tempdir().unwrap();
    let running = Running::start(root.path(), &["--pair"]);
    let code = running.line_with("OAI1");
    let invite = InviteCode::decode(code.trim()).unwrap();
    running.line_with("Press Ctrl+C to stop.");

    let quiet: Logger = std::sync::Arc::new(|_, _| {});
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let client = runtime.block_on(async {
        let endpoint = bind(new_device_key()).await.unwrap();
        let paired = redeem(&endpoint, &invite, "Test phone", Some("Model"), &quiet)
            .await
            .unwrap();
        let client = RemoteClient::new(endpoint, paired.id, quiet.clone());
        client.connection().await.unwrap();
        client
    });
    running.line_with("Test phone (Model) was paired");
    running.line_with("Test phone (Model) connected");

    let (ok, text) = output(root.path(), &["devices"]);
    assert!(
        ok && text.contains("1  Test phone (Model)  ·  connected"),
        "{text}"
    );
    let (ok, text) = output(root.path(), &["remove", "1", "--yes"]);
    assert!(ok && text.contains("Removed Test phone (Model)."), "{text}");
    running.line_with("Test phone (Model) was removed");

    runtime.block_on(client.disconnect());
    running.interrupt();
    assert!(running.stopped());
}
