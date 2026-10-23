use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::*;

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const VERSION: &str = "0.11.0";
const LOCAL: &str = "127.0.0.1:50001";

/// Answers each command with the next scripted reply after checking that the
/// command is the expected one.
struct FakeSsh {
    replies: VecDeque<(&'static str, Result<String, TransportError>)>,
    uploaded: Option<Vec<u8>>,
    forwarded: Option<u16>,
}

impl FakeSsh {
    fn new(replies: Vec<(&'static str, Result<String, TransportError>)>) -> Self {
        Self {
            replies: replies.into(),
            uploaded: None,
            forwarded: None,
        }
    }
}

impl SshTransport for FakeSsh {
    type Forward = u16;

    fn run(
        &mut self,
        command: &str,
        input: Option<&mut dyn Read>,
    ) -> Result<RemoteOutput, TransportError> {
        let (expected, reply) = self.replies.pop_front().expect("an unexpected command");
        assert!(
            command.contains(expected),
            "expected {expected:?} in {command}"
        );
        if let Some(input) = input {
            let mut bytes = Vec::new();
            input.read_to_end(&mut bytes).unwrap();
            self.uploaded = Some(bytes);
        }
        reply.map(|stdout| RemoteOutput {
            success: true,
            stdout: stdout.into_bytes(),
        })
    }

    fn forward(&mut self, remote_port: u16) -> Result<(u16, SocketAddr), TransportError> {
        self.forwarded = Some(remote_port);
        Ok((remote_port, LOCAL.parse().unwrap()))
    }
}

type Events = Arc<Mutex<Vec<(&'static str, Value)>>>;

fn launcher() -> (SshLauncher, Events, Arc<Mutex<Vec<Stage>>>) {
    let events: Events = Arc::default();
    let stages: Arc<Mutex<Vec<Stage>>> = Arc::default();
    let sink = events.clone();
    let seen = stages.clone();
    let launcher = SshLauncher::new(
        ServerRelease::new(VERSION, SHA).unwrap(),
        Arc::new(move |name, fields| sink.lock().unwrap().push((name, fields))),
        Box::new(move |stage| seen.lock().unwrap().push(stage)),
    );
    (launcher, events, stages)
}

fn probe(installed: &str) -> (&'static str, Result<String, TransportError>) {
    (
        "uname -sm",
        Ok(format!(
            "platform=Linux x86_64\ninstalled={installed}\nkill_user_processes=yes\nlinger=no\n"
        )),
    )
}

fn report(outcome: &str, app_version: &str, protocol: &str) -> Result<String, TransportError> {
    Ok(json!({
        "outcome": outcome,
        "keeper": "new",
        "endpointUrl": "http://127.0.0.1:43123/rpc",
        "authToken": "secret-token",
        "appVersion": app_version,
        "protocolVersion": protocol,
    })
    .to_string())
}

fn no_fetch() -> Result<Box<dyn Read>, &'static str> {
    panic!("the installed binary should have been used")
}

fn ready(launched: Result<Launched<u16>, LaunchError>) -> Ready<u16> {
    match launched {
        Ok(Launched::Ready(ready)) => ready,
        Ok(Launched::UpdateRequired { running_version }) => {
            panic!("update required from {running_version}")
        }
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn installs_a_missing_release_then_forwards_the_kept_app_server() {
    let (launcher, events, stages) = launcher();
    let mut ssh = FakeSsh::new(vec![
        probe(""),
        ("openaide-app-server.upload", Ok("installed\n".into())),
        (
            "keep \"$@\"' openaide '0.11.0' 'start'",
            report("started", VERSION, "2.2"),
        ),
    ]);
    let mut fetched = 0;
    let mut fetch = || -> Result<Box<dyn Read>, &'static str> {
        fetched += 1;
        Ok(Box::new(&b"binary"[..]))
    };

    let ready = ready(launcher.launch(&mut ssh, StartMode::Keep, &mut fetch, "op-1"));

    assert_eq!(fetched, 1);
    assert_eq!(ssh.uploaded.as_deref(), Some(&b"binary"[..]));
    assert_eq!(ssh.forwarded, Some(43123));
    assert_eq!(ready.endpoint_url, format!("http://{LOCAL}/rpc"));
    assert_eq!(ready.auth_token, "secret-token");
    assert!(!ready.update_available && !ready.keeper_existed);
    assert!(ready.suggest_linger);
    assert_eq!(
        *stages.lock().unwrap(),
        [
            Stage::Probe,
            Stage::Fetch,
            Stage::Upload,
            Stage::Start,
            Stage::Forward
        ]
    );
    let events = events.lock().unwrap();
    let (name, completed) = events.last().unwrap();
    assert_eq!(*name, "ssh_launch_completed");
    assert_eq!(completed["outcome"], "ready");
    assert_eq!(completed["operationId"], "op-1");
    assert!(!format!("{events:?}").contains("secret-token"));
}

#[test]
fn an_installed_release_is_not_fetched_again() {
    let (launcher, _, stages) = launcher();
    let mut ssh = FakeSsh::new(vec![
        probe(SHA),
        ("keep", report("attached", VERSION, "2.2")),
    ]);

    ready(launcher.launch(&mut ssh, StartMode::Keep, &mut no_fetch, "op-1"));

    assert_eq!(
        *stages.lock().unwrap(),
        [Stage::Probe, Stage::Start, Stage::Forward]
    );
}

#[test]
fn an_older_app_server_that_speaks_the_protocol_is_joined_and_offered_an_update() {
    let (launcher, _, _) = launcher();
    let mut ssh = FakeSsh::new(vec![
        probe(SHA),
        ("'start'", report("mismatch", "0.10.1", "2.2")),
        ("'start' '--join'", report("joined", "0.10.1", "2.2")),
    ]);

    let ready = ready(launcher.launch(&mut ssh, StartMode::Keep, &mut no_fetch, "op-1"));

    assert!(ready.update_available);
    assert_eq!(ready.app_version, "0.10.1");
}

#[test]
fn an_older_app_server_this_shell_cannot_use_needs_a_confirmed_update() {
    let (launcher, _, _) = launcher();
    let mut ssh = FakeSsh::new(vec![
        probe(SHA),
        ("'start'", report("mismatch", "0.10.0", "2.1")),
    ]);

    let launched = launcher.launch(&mut ssh, StartMode::Keep, &mut no_fetch, "op-1");

    assert!(matches!(
        launched,
        Ok(Launched::UpdateRequired { ref running_version }) if running_version == "0.10.0"
    ));
    assert_eq!(ssh.forwarded, None);

    let mut ssh = FakeSsh::new(vec![
        probe(SHA),
        ("'start' '--replace'", report("started", VERSION, "2.2")),
    ]);
    ready(launcher.launch(&mut ssh, StartMode::Replace, &mut no_fetch, "op-2"));
}

#[test]
fn a_newer_app_server_is_never_replaced() {
    let (launcher, events, _) = launcher();
    let mut ssh = FakeSsh::new(vec![
        probe(SHA),
        ("'start'", report("mismatch", "0.12.0", "2.3")),
    ]);

    let error = launcher
        .launch(&mut ssh, StartMode::Keep, &mut no_fetch, "op-1")
        .err()
        .unwrap();

    assert_eq!(
        error,
        LaunchError::ServerNewer {
            running_version: "0.12.0".into()
        }
    );
    let events = events.lock().unwrap();
    let (_, completed) = events.last().unwrap();
    assert_eq!(completed["stage"], "start");
    assert_eq!(completed["errorClass"], "server_newer");
}

#[test]
fn failures_are_classified_by_stage() {
    let (launcher, _, _) = launcher();
    let unsupported = FakeSsh::new(vec![("uname", Ok("platform=Darwin arm64\n".into()))]);
    let unreachable = FakeSsh::new(vec![(
        "uname",
        Err(TransportError {
            class: "unreachable",
        }),
    )]);
    let corrupted = FakeSsh::new(vec![
        probe(""),
        ("upload", Ok("checksum_mismatch\n".into())),
    ]);
    let busy = FakeSsh::new(vec![
        probe(SHA),
        (
            "keep",
            Ok(r#"{"outcome":"failed","error":"keeper_busy"}"#.into()),
        ),
    ]);
    let elsewhere = FakeSsh::new(vec![
        probe(SHA),
        (
            "keep",
            Ok(
                r#"{"outcome":"started","endpointUrl":"http://10.0.0.5:1/","authToken":"t"}"#
                    .into(),
            ),
        ),
    ]);
    let mut fetch = || -> Result<Box<dyn Read>, &'static str> { Ok(Box::new(&b""[..])) };

    let mut classes = Vec::new();
    for mut ssh in [unsupported, unreachable, corrupted, busy, elsewhere] {
        let error = launcher
            .launch(&mut ssh, StartMode::Keep, &mut fetch, "op")
            .err()
            .unwrap();
        classes.push((error.stage(), error.class().to_string()));
    }

    assert_eq!(
        classes,
        [
            (Stage::Probe, "unsupported_platform".to_string()),
            (Stage::Probe, "unreachable".to_string()),
            (Stage::Upload, "checksum_mismatch".to_string()),
            (Stage::Start, "keeper_busy".to_string()),
            (Stage::Start, "unreadable_answer".to_string()),
        ]
    );
}

#[test]
fn stop_reports_what_the_keeper_did() {
    let (launcher, events, _) = launcher();
    let mut ssh = FakeSsh::new(vec![("'stop'", Ok(r#"{"outcome":"stopped"}"#.into()))]);

    assert_eq!(launcher.stop(&mut ssh, "op-3"), Ok(StopOutcome::Stopped));
    let events = events.lock().unwrap();
    assert_eq!(events[0].0, "ssh_stop_completed");
    assert_eq!(events[0].1["outcome"], "stopped");
}

#[test]
fn a_release_must_be_safe_to_name_in_a_command() {
    assert!(ServerRelease::new("0.11.0", SHA).is_some());
    assert!(ServerRelease::new("0.11.0 && true", SHA).is_none());
    assert!(ServerRelease::new("0.11.0", "abc").is_none());
}
