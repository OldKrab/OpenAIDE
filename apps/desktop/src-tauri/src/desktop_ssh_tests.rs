use openaide_remote_client::TransportError;

use super::*;

#[test]
fn failures_name_the_computer_and_never_the_raw_error() {
    let failure = LaunchError::Transport {
        stage: Stage::Probe,
        error: TransportError {
            class: "auth_failed",
        },
    };

    assert_eq!(
        message(&failure, "devbox"),
        "devbox did not accept the sign-in. Check your SSH key or password."
    );
    assert!(
        message(
            &LaunchError::ServerNewer {
                running_version: "0.12.0".into()
            },
            "devbox"
        )
        .contains("0.12.0")
    );
    assert!(
        message(&LaunchError::Install("checksum_tool_missing"), "devbox").contains("sha256sum")
    );
}

/// Drives the system `ssh` and the shared launcher against a real Linux x64
/// host: install, start, forward, attach again, and stop. Run it by hand with
/// `OPENAIDE_SSH_E2E_DESTINATION` (a host `ssh` reaches without prompts),
/// `OPENAIDE_SSH_E2E_BINARY` (a Linux x64 `openaide-app-server`), and
/// `OPENAIDE_SSH_E2E_VERSION` (that binary's version), then
/// `cargo test against_a_real_ssh_host -- --ignored`.
#[test]
#[ignore = "needs an SSH host; see the doc comment"]
fn against_a_real_ssh_host() {
    use std::io::{Read, Write};

    use openaide_remote_client::ServerRelease;
    use sha2::{Digest, Sha256};

    let variable = |name| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let destination = variable("OPENAIDE_SSH_E2E_DESTINATION");
    let binary = std::fs::read(variable("OPENAIDE_SSH_E2E_BINARY")).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(&binary));
    let release = ServerRelease::new(&variable("OPENAIDE_SSH_E2E_VERSION"), &sha256).unwrap();
    let launcher = SshLauncher::new(release, crate::desktop_remote::logger(), Box::new(|_| {}));
    let mut transport = SystemSsh::new(&destination, Vec::new());
    let launch = |transport: &mut SystemSsh, operation_id: &str| {
        let mut fetch = || Ok(Box::new(std::io::Cursor::new(binary.clone())) as Box<dyn Read>);
        match launcher.launch(transport, StartMode::Keep, &mut fetch, operation_id) {
            Ok(Launched::Ready(ready)) => ready,
            Ok(Launched::UpdateRequired { running_version }) => {
                panic!("the host runs an incompatible {running_version}")
            }
            Err(error) => panic!("{} failed: {}", error.stage().as_str(), error.class()),
        }
    };

    let first = launch(&mut transport, "e2e-1");
    let url = url::Url::parse(&first.endpoint_url).unwrap();
    let mut stream = std::net::TcpStream::connect((
        url.host_str().unwrap(),
        url.port_or_known_default().unwrap(),
    ))
    .unwrap();
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        url.path()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 "), "{response}");

    let second = launch(&mut transport, "e2e-2");
    assert!(second.keeper_existed);
    assert_eq!(second.auth_token, first.auth_token);
    drop((first, second));

    assert_eq!(
        launcher.stop(&mut transport, "e2e-3").unwrap(),
        StopOutcome::Stopped
    );
}
