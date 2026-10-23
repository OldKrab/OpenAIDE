use super::*;

const SEED: [u8; 32] = [7; 32];

#[test]
fn the_public_key_line_is_an_ed25519_authorized_keys_entry() {
    let line = public_key_line(SEED.to_vec(), "openaide-android".into()).unwrap();

    assert!(line.starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5"));
    assert!(line.ends_with(" openaide-android"));
    assert!(is_key_line(&line));
    assert_eq!(public_key_line(SEED.to_vec(), "openaide-android".into()).unwrap(), line);
}

#[test]
fn a_key_line_that_could_leave_its_quotes_is_refused() {
    assert!(!is_key_line("ssh-ed25519 AAAA x'; rm -rf ~"));
    assert!(!is_key_line("ssh-rsa AAAA x"));
    assert!(matches!(
        public_key_line(vec![1; 31], String::new()),
        Err(SshError::Failed { class, .. }) if class == "key_invalid"
    ));
}

#[test]
fn fingerprints_are_openssh_sha256() {
    let line = public_key_line(SEED.to_vec(), String::new()).unwrap();
    let fingerprint = host_key_fingerprint(line).unwrap();

    assert!(fingerprint.starts_with("SHA256:"));
    assert_eq!(fingerprint.len(), "SHA256:".len() + 43);
    assert!(host_key_fingerprint("not a key".into()).is_err());
}

struct PrintedEvents {
    archive: Option<String>,
}

impl SshEvents for PrintedEvents {
    fn log(&self, event: String, fields: String) {
        println!("{event} {fields}");
    }

    fn stage(&self, _stage: String) {}

    fn release_archive(&self) -> Result<String, SshError> {
        self.archive
            .clone()
            .ok_or_else(|| SshError::new("fetch", "release_not_published"))
    }
}

/// The phone's whole flow against a real Linux x64 host: read and accept the
/// host key, add the phone's key with a password once, launch, reach the App
/// Server through the forward, attach again, and stop. Run it by hand with
/// `OPENAIDE_SSH_E2E_HOST`, `_PORT`, `_USER`, `_PASSWORD`, `_ARCHIVE` (a
/// published Linux x64 archive or one packed the same way), `_SHA256` (its
/// binary's), and `_VERSION`, then
/// `cargo test -p openaide-android-ssh against_a_real_ssh_host -- --ignored`.
#[test]
#[ignore = "needs an SSH host; see the doc comment"]
fn against_a_real_ssh_host() {
    use std::io::{Read, Write};

    let variable = |name: &str| {
        std::env::var(format!("OPENAIDE_SSH_E2E_{name}"))
            .unwrap_or_else(|_| panic!("OPENAIDE_SSH_E2E_{name} is not set"))
    };
    let target = Target {
        host: variable("HOST"),
        port: variable("PORT").parse().unwrap(),
        user: variable("USER"),
    };
    let seed = [42; 32].to_vec();
    let cache = tempfile::tempdir().unwrap();
    let archive = cache.path().join("download.tar.gz");
    std::fs::copy(variable("ARCHIVE"), &archive).unwrap();
    let events: Arc<dyn SshEvents> = Arc::new(PrintedEvents {
        archive: Some(archive.to_string_lossy().into_owned()),
    });

    let host_key = read_host_key(target.clone()).unwrap();
    println!("host key {}", host_key_fingerprint(host_key.clone()).unwrap());
    let line = public_key_line(seed.clone(), "openaide-android".into()).unwrap();
    authorize_key(
        target.clone(),
        host_key.clone(),
        variable("PASSWORD"),
        line,
        events.clone(),
    )
    .unwrap();

    let computer = SshComputer::new(
        target.clone(),
        host_key.clone(),
        seed.clone(),
        variable("VERSION"),
        variable("SHA256"),
        cache.path().join("release").to_string_lossy().into_owned(),
        events.clone(),
    )
    .unwrap();
    let SshLaunch::Ready { ready } = computer.launch(false, "e2e-1".into()).unwrap() else {
        panic!("the host runs an incompatible App Server");
    };
    assert!(!ready.keeper_existed);
    let address = ready
        .endpoint_url
        .strip_prefix("http://")
        .and_then(|rest| rest.split_once('/'))
        .unwrap();
    for _ in 0..2 {
        // Each request is its own connection and so its own channel.
        let mut stream = std::net::TcpStream::connect(address.0).unwrap();
        write!(
            stream,
            "GET /{} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            address.1
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 "), "{response}");
    }
    computer.disconnect();

    let again = SshComputer::new(
        target.clone(),
        host_key.clone(),
        seed.clone(),
        variable("VERSION"),
        variable("SHA256"),
        cache.path().join("release").to_string_lossy().into_owned(),
        events.clone(),
    )
    .unwrap();
    let SshLaunch::Ready { ready } = again.launch(false, "e2e-2".into()).unwrap() else {
        panic!("the second launch did not attach");
    };
    assert!(ready.keeper_existed);
    assert_eq!(again.stop("e2e-3".into()).unwrap(), "stopped");
    again.disconnect();

    let other_key = public_key_line([1; 32].to_vec(), String::new()).unwrap();
    let wrong = SshComputer::new(
        target,
        other_key,
        seed,
        variable("VERSION"),
        variable("SHA256"),
        cache.path().join("release").to_string_lossy().into_owned(),
        events,
    )
    .unwrap();
    assert!(matches!(
        wrong.launch(false, "e2e-4".into()),
        Err(SshError::Failed { class, .. }) if class == "host_key_changed"
    ));
}
