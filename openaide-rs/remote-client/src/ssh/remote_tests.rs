use std::process::Command;

use super::*;

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn scripts_survive_every_login_shell_unchanged() {
    for script in [INSTALL_DIRECTORY, PROBE, INSTALL, KEEP] {
        assert!(!script.contains(['\'', '!', '\n']), "{script}");
    }
    assert_eq!(quote("it's"), r"'it'\''s'");
}

/// Runs a generated command line the way an SSH server does, through a shell,
/// against a temporary home.
fn run(line: &str, home: &std::path::Path, input: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(line)
        .env("HOME", home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap().stdout
}

#[cfg(unix)]
#[test]
fn installs_only_a_binary_whose_checksum_matches() {
    let home = tempfile::tempdir().unwrap();
    let binary = b"#!/bin/sh\necho \"$*\"\n";
    let sha = sha256sum(binary);

    let rejected = run(&install("1.2.3", SHA), home.path(), binary);
    assert_eq!(install_verdict(&rejected), Err("checksum_mismatch"));
    let installed = home
        .path()
        .join(".local/share/openaide/app-server/1.2.3/openaide-app-server");
    assert!(!installed.exists());

    let accepted = run(&install("1.2.3", &sha), home.path(), binary);
    assert_eq!(install_verdict(&accepted), Ok(()));
    assert_eq!(std::fs::read(&installed).unwrap(), binary);

    let probe = Probe::parse(&run(&probe("1.2.3"), home.path(), b""));
    assert_eq!(probe.installed_sha256.as_deref(), Some(sha.as_str()));
    assert!(!probe.platform.is_empty());

    let kept = run(&keep("1.2.3", &["start", "--join"]), home.path(), b"");
    assert_eq!(String::from_utf8_lossy(&kept), "keep start --join\n");
}

#[cfg(unix)]
fn sha256sum(bytes: &[u8]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("payload");
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new("sha256sum").arg(&path).output().unwrap();
    String::from_utf8(output.stdout).unwrap()[..64].to_string()
}

#[test]
fn probe_reads_logind_facts() {
    let probe =
        Probe::parse(b"platform=Linux x86_64\ninstalled=ABC\nkill_user_processes=yes\nlinger=no\n");

    assert_eq!(
        probe,
        Probe {
            platform: "Linux x86_64".into(),
            installed_sha256: Some("abc".into()),
            kill_user_processes: Some(true),
            linger: Some(false),
        }
    );
    assert!(probe.is_supported());
    assert!(probe.suggests_linger());
    assert!(!Probe::parse(b"platform=Linux aarch64\nkill_user_processes=\n").is_supported());
    assert!(!Probe::parse(b"platform=Linux x86_64\nkill_user_processes=\n").suggests_linger());
}

#[test]
fn keeper_report_is_the_last_line_and_its_error_is_a_class() {
    let report =
        KeeperReport::parse(b"motd noise\n{\"outcome\":\"failed\",\"error\":\"keeper_busy\"}\n\n")
            .unwrap();
    assert_eq!(report.outcome, "failed");
    assert_eq!(report.error_class(), "keeper_busy");

    let odd =
        KeeperReport::parse(br#"{"outcome":"failed","error":"/home/someone: denied"}"#).unwrap();
    assert_eq!(odd.error_class(), "unrecognized");
    assert!(KeeperReport::parse(b"sh: not found\n").is_none());
}

#[test]
fn only_a_loopback_endpoint_is_forwarded() {
    assert_eq!(
        RemoteEndpoint::parse("http://127.0.0.1:43123/rpc"),
        Some(RemoteEndpoint {
            port: 43123,
            path: "/rpc".into()
        })
    );
    assert_eq!(
        RemoteEndpoint::parse("http://[::1]:8080").map(|endpoint| endpoint.path),
        Some("/".into())
    );
    assert_eq!(RemoteEndpoint::parse("http://10.0.0.5:43123/rpc"), None);
    assert_eq!(RemoteEndpoint::parse("https://127.0.0.1:43123/"), None);
    assert_eq!(RemoteEndpoint::parse("http://127.0.0.1:0/"), None);
}

#[test]
fn only_inert_versions_and_checksums_are_accepted() {
    assert!(is_version("0.11.0-rc.1+build.5"));
    assert!(!is_version("0.11.0; rm -rf ~"));
    assert!(!is_version(""));
    assert!(is_sha256(SHA));
    assert!(!is_sha256(&SHA.to_uppercase()));
    assert!(!is_sha256(&SHA[1..]));
}
