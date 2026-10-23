//! Drives the shared SSH launcher against this machine: every command runs
//! through `sh` the way an SSH server runs it, and the forward is the identity,
//! so the release binary built here is installed, kept and stopped for real.

#![cfg(target_os = "linux")]

use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use openaide_remote_client::{
    LaunchError, Launched, RemoteOutput, ServerRelease, SshLauncher, SshTransport, StartMode,
    StopOutcome, TransportError,
};

const PROGRAM: &str = env!("CARGO_BIN_EXE_openaide-app-server");
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Runs commands in a temporary home with its own state and runtime roots.
struct LocalShell {
    home: PathBuf,
}

impl SshTransport for LocalShell {
    type Forward = ();

    fn run(
        &mut self,
        command: &str,
        input: Option<&mut dyn Read>,
    ) -> Result<RemoteOutput, TransportError> {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("HOME", &self.home)
            .env("OPENAIDE_STORAGE_ROOT", self.home.join("state"))
            .env("OPENAIDE_RUNTIME_ROOT", self.home.join("run"))
            .env_remove("OPENAIDE_APP_SERVER_PROTOCOL")
            .env_remove("OPENAIDE_RUNTIME_PROTOCOL")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| TransportError { class: "spawn" })?;
        let mut stdin = child.stdin.take().unwrap();
        if let Some(input) = input {
            std::io::copy(input, &mut stdin).map_err(|_| TransportError { class: "upload" })?;
        }
        drop(stdin);
        let output = child
            .wait_with_output()
            .map_err(|_| TransportError { class: "wait" })?;
        Ok(RemoteOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }

    fn forward(&mut self, remote_port: u16) -> Result<((), SocketAddr), TransportError> {
        Ok(((), SocketAddr::from(([127, 0, 0, 1], remote_port))))
    }
}

/// Lets a kept App Server go if an assertion fails first.
struct StopKeeper<'a>(&'a SshLauncher, &'a Path);

impl Drop for StopKeeper<'_> {
    fn drop(&mut self) {
        let mut shell = LocalShell {
            home: self.1.to_path_buf(),
        };
        let _ = self.0.stop(&mut shell, "cleanup");
    }
}

fn sha256(path: &str) -> String {
    let output = Command::new("sha256sum").arg(path).output().unwrap();
    String::from_utf8(output.stdout).unwrap()[..64].to_string()
}

fn launch(launcher: &SshLauncher, home: &Path) -> Result<Launched<()>, LaunchError> {
    let mut shell = LocalShell {
        home: home.to_path_buf(),
    };
    let mut fetch = || -> Result<Box<dyn Read>, &'static str> {
        Ok(Box::new(std::fs::File::open(PROGRAM).unwrap()))
    };
    launcher.launch(&mut shell, StartMode::Keep, &mut fetch, "test")
}

#[test]
fn installs_keeps_and_stops_the_release_through_a_shell() {
    let home = tempfile::tempdir().unwrap();
    let launcher = SshLauncher::new(
        ServerRelease::new(VERSION, &sha256(PROGRAM)).unwrap(),
        Arc::new(|_, _| {}),
        Box::new(|_| {}),
    );
    let _cleanup = StopKeeper(&launcher, home.path());

    let Ok(Launched::Ready(first)) = launch(&launcher, home.path()) else {
        panic!("the first launch did not reach a ready App Server");
    };
    assert!(!first.keeper_existed);
    assert_eq!(first.app_version, VERSION);
    assert!(home
        .path()
        .join(format!(
            ".local/share/openaide/app-server/{VERSION}/openaide-app-server"
        ))
        .is_file());

    let Ok(Launched::Ready(second)) = launch(&launcher, home.path()) else {
        panic!("the second launch did not reach a ready App Server");
    };
    assert!(second.keeper_existed);
    assert_eq!(second.endpoint_url, first.endpoint_url);
    assert_eq!(second.auth_token, first.auth_token);

    let mut shell = LocalShell {
        home: home.path().to_path_buf(),
    };
    assert_eq!(launcher.stop(&mut shell, "test"), Ok(StopOutcome::Stopped));
}
