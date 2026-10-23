//! `openaide-app-server keep`: the keeper an App Shell starts over SSH on an SSH
//! Computer (ADR-0069).
//!
//! `keep start` runs in the SSH session and returns at once. It starts `keep run`
//! in a session of its own, relays the one line `keep run` prints about the App
//! Server, and exits. `keep run` then stays as a windowless client of kind
//! `keeper` and holds the App Server while [`policy`] says so. The App Server's
//! lifetime rule does not change: once the keeper and every other client have
//! left, it stops.
//!
//! The printed line is the launcher's only interface: one JSON object whose
//! `outcome` is `started`, `attached`, `joined`, `mismatch`, or `failed`.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openaide_app_server::logging;
use openaide_app_server::storage_runtime::StateRoot;
use openaide_app_server_protocol::client::{
    ClientReplacementImpactResult, ShellKind, APP_SERVER_PROTOCOL_VERSION,
};
use openaide_app_server_protocol::methods::CLIENT_REPLACEMENT_IMPACT;
use serde::Serialize;
use serde_json::json;

use crate::app_server_handoff::{self, LocalHttpHandoffConnection, ReportingHandoff};
use crate::remote_command::launch;
use crate::remote_command::local_client::{ClientError, LocalClient};

mod lease;
mod policy;

use lease::{LeaseAttempt, LeaseHolder};
use policy::{KeeperClock, KeeperPolicy, Observation, Verdict};

const TICK: Duration = Duration::from_millis(100);
/// Matches the `remote` command: well inside the App Server's liveness window.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
/// The policy counts in minutes and hours; reading more often adds nothing.
const OBSERVE_INTERVAL: Duration = Duration::from_secs(30);
const MAX_MISSED_HEARTBEATS: u32 = 5;
/// How long a replaced App Server's keeper gets to leave before the new one gives up.
const LEASE_TAKEOVER_WAIT: Duration = Duration::from_secs(15);
const STOP_WAIT: Duration = Duration::from_secs(10);

const USAGE: &str = "\
Usage (run by an OpenAIDE App Shell over SSH):
  openaide-app-server keep start            Keep an App Server of this version running; never replace another version
  openaide-app-server keep start --replace  Replace an App Server of another version, then keep it
  openaide-app-server keep start --join     Keep whatever App Server runs, whatever its version
  openaide-app-server keep stop             Stop the keeper; the App Server stops once its last client leaves
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Attach to or start this version; report another version untouched.
    Own,
    Replace,
    Join,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Start(Mode),
    Run(Mode),
    Stop,
    Help,
}

pub(crate) fn parse(arguments: &[String]) -> Result<Action, String> {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let mode = |rest: &[&str]| match rest {
        [] => Ok(Mode::Own),
        ["--replace"] => Ok(Mode::Replace),
        ["--join"] => Ok(Mode::Join),
        _ => Err(format!("Unexpected \"{}\".", rest.join(" "))),
    };
    match words.split_first() {
        Some((&"start", rest)) => mode(rest).map(Action::Start),
        Some((&"run", rest)) => mode(rest).map(Action::Run),
        Some((&"stop", [])) => Ok(Action::Stop),
        Some((&("help" | "--help" | "-h"), _)) | None => Ok(Action::Help),
        Some((word, _)) => Err(format!("Unknown command \"{word}\".")),
    }
}

/// What `keep start` prints. Tokens and addresses are for the launcher only and
/// are never logged.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub(crate) outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) keeper: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) app_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) protocol_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<&'static str>,
}

impl Report {
    fn failed(error: &'static str) -> Self {
        Self {
            outcome: "failed",
            error: Some(error),
            ..Self::default()
        }
    }

    fn connected(
        outcome: &'static str,
        connection: &LocalHttpHandoffConnection,
        app_version: String,
        protocol_version: String,
    ) -> Self {
        Self {
            outcome,
            endpoint_url: Some(connection.endpoint_url().to_string()),
            auth_token: Some(connection.auth_token().to_string()),
            app_version: Some(app_version),
            protocol_version: Some(protocol_version),
            ..Self::default()
        }
    }
}

pub(crate) fn run(arguments: &[String], state_root: &StateRoot, runtime_root: &Path) -> i32 {
    let action = match parse(arguments) {
        Ok(action) => action,
        Err(problem) => {
            eprintln!("{problem}\n\n{USAGE}");
            return 2;
        }
    };
    match action {
        Action::Help => {
            print!("{USAGE}");
            0
        }
        Action::Start(mode) => start(mode),
        Action::Stop => stop(state_root, runtime_root),
        Action::Run(mode) => keep(mode, state_root, runtime_root),
    }
}

/// Starts `keep run` outside this SSH session and relays its one line.
fn start(mode: Mode) -> i32 {
    let started = Instant::now();
    logging::info("keeper_start_started", json!({ "mode": mode_class(mode) }));
    let relayed = spawn_keeper(mode).and_then(|line| {
        println!("{line}");
        std::io::stdout().flush().map_err(|_| "output_closed")
    });
    let outcome = relayed.err().unwrap_or("relayed");
    logging::info(
        "keeper_start_completed",
        json!({ "mode": mode_class(mode), "outcome": outcome, "duration_ms": elapsed_ms(started) }),
    );
    if outcome == "relayed" {
        0
    } else {
        println!(
            "{}",
            serde_json::to_string(&Report::failed(outcome)).unwrap_or_default()
        );
        1
    }
}

fn spawn_keeper(mode: Mode) -> Result<String, &'static str> {
    let executable = std::env::current_exe().map_err(|_| "spawn_failed")?;
    let mut command = Command::new(executable);
    command.arg("keep").arg("run");
    match mode {
        Mode::Own => {}
        Mode::Replace => {
            command.arg("--replace");
        }
        Mode::Join => {
            command.arg("--join");
        }
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    new_session(&mut command);
    let mut child = command.spawn().map_err(|_| "spawn_failed")?;
    let stdout = child.stdout.take().ok_or("no_report")?;
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .map_err(|_| "no_report")?;
    let line = line.trim().to_string();
    if line.is_empty() {
        return Err("no_report");
    }
    // The keeper outlives this command; nothing waits for it.
    drop(child);
    Ok(line)
}

/// The keeper must survive the SSH session that started it.
#[cfg(unix)]
fn new_session(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `setsid` is async-signal-safe and touches no memory of the parent.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(std::io::Error::from)
        });
    }
}

#[cfg(not(unix))]
fn new_session(_command: &mut Command) {}

/// The detached keeper: resolves the App Server, takes the lease, reports, keeps.
fn keep(mode: Mode, state_root: &StateRoot, runtime_root: &Path) -> i32 {
    let started = Instant::now();
    let resolved = resolve(mode, state_root, runtime_root);
    let (outcome, connection, app_version, protocol_version, _launcher) = match resolved {
        Ok(Resolved::Keep {
            outcome,
            connection,
            app_version,
            protocol_version,
            launcher,
        }) => (outcome, connection, app_version, protocol_version, launcher),
        Ok(Resolved::Mismatch(report)) => {
            logging::info(
                "keeper_not_started",
                json!({ "mode": mode_class(mode), "outcome": report.outcome, "duration_ms": elapsed_ms(started) }),
            );
            print_report(&report);
            return 0;
        }
        Err(error) => {
            let report = Report::failed(error);
            logging::info(
                "keeper_not_started",
                json!({ "mode": mode_class(mode), "outcome": report.outcome, "error": report.error, "duration_ms": elapsed_ms(started) }),
            );
            print_report(&report);
            return 0;
        }
    };

    let path = lease::lease_path(runtime_root, state_root.fingerprint().as_str());
    let holder = LeaseHolder {
        pid: std::process::id(),
        server_id: connection.server_id().to_string(),
    };
    let lease = match take_lease(&path, &holder) {
        Ok(Some(lease)) => lease,
        Ok(None) => {
            let mut report = Report::connected(outcome, &connection, app_version, protocol_version);
            report.keeper = Some("existing");
            logging::info(
                "keeper_already_running",
                json!({ "outcome": outcome, "duration_ms": elapsed_ms(started) }),
            );
            print_report(&report);
            return 0;
        }
        Err(error) => {
            print_report(&Report::failed(error));
            return 1;
        }
    };

    let mut client = match LocalClient::with_kind(
        ShellKind::Keeper,
        connection.endpoint_url(),
        connection.auth_token(),
    )
    .and_then(|mut client| client.initialize().map(|()| client))
    {
        Ok(client) => client,
        Err(error) => {
            print_report(&Report::failed(error.class()));
            return 1;
        }
    };
    let mut report = Report::connected(outcome, &connection, app_version, protocol_version);
    report.keeper = Some("new");
    print_report(&report);
    logging::info(
        "keeper_started",
        json!({ "mode": mode_class(mode), "outcome": outcome, "duration_ms": elapsed_ms(started) }),
    );
    let ended = hold(&mut client, KeeperPolicy::default());
    let _ = client.detach();
    drop(lease);
    logging::info(
        "keeper_ended",
        json!({ "reason": ended, "held_ms": elapsed_ms(started) }),
    );
    0
}

enum Resolved {
    Keep {
        outcome: &'static str,
        connection: LocalHttpHandoffConnection,
        app_version: String,
        protocol_version: String,
        /// Holds the output pipe of an App Server this keeper started.
        launcher: Option<launch::Connection>,
    },
    Mismatch(Report),
}

fn own_versions() -> (String, String) {
    (
        env!("CARGO_PKG_VERSION").to_string(),
        APP_SERVER_PROTOCOL_VERSION.to_string(),
    )
}

fn resolve(
    mode: Mode,
    state_root: &StateRoot,
    runtime_root: &Path,
) -> Result<Resolved, &'static str> {
    if mode == Mode::Replace {
        // The ordinary App Shell path: attach to this version, or ask another one
        // to stop and start this one. The user has confirmed.
        let launched =
            launch::attach_or_start(state_root, runtime_root).map_err(|error| error.class())?;
        return keep_own(launched, state_root, runtime_root);
    }
    match app_server_handoff::attach_or_launch_reporting(state_root, runtime_root)? {
        ReportingHandoff::Attached(connection) => {
            let (app_version, protocol_version) = own_versions();
            Ok(Resolved::Keep {
                outcome: "attached",
                connection,
                app_version,
                protocol_version,
                launcher: None,
            })
        }
        ReportingHandoff::LaunchHere(lock) if mode == Mode::Own => {
            // The started process takes the launch itself.
            drop(lock);
            let launched = launch::start().map_err(|error| error.class())?;
            keep_own(launched, state_root, runtime_root)
        }
        ReportingHandoff::LaunchHere(_lock) => Err("not_running"),
        ReportingHandoff::Incompatible(server) if mode == Mode::Join => Ok(Resolved::Keep {
            outcome: "joined",
            connection: server.connection,
            app_version: server.app_version,
            protocol_version: server.protocol_version,
            launcher: None,
        }),
        ReportingHandoff::Incompatible(server) => Ok(Resolved::Mismatch(Report::connected(
            "mismatch",
            &server.connection,
            server.app_version,
            server.protocol_version,
        ))),
    }
}

/// An App Server of this version now runs; reading its endpoint record again
/// gives the server id the lease names.
fn keep_own(
    launched: launch::Connection,
    state_root: &StateRoot,
    runtime_root: &Path,
) -> Result<Resolved, &'static str> {
    let outcome = if launched.started {
        "started"
    } else {
        "attached"
    };
    match app_server_handoff::attach_or_launch_reporting(state_root, runtime_root)? {
        ReportingHandoff::Attached(connection) => {
            let (app_version, protocol_version) = own_versions();
            Ok(Resolved::Keep {
                outcome,
                connection,
                app_version,
                protocol_version,
                launcher: Some(launched),
            })
        }
        _ => Err("launch_unconfirmed"),
    }
}

/// `Ok(None)` when a keeper already keeps this App Server.
fn take_lease(path: &Path, holder: &LeaseHolder) -> Result<Option<lease::Lease>, &'static str> {
    let deadline = Instant::now() + LEASE_TAKEOVER_WAIT;
    loop {
        match lease::try_acquire(path, holder).map_err(|_| "lease_failed")? {
            LeaseAttempt::Acquired(lease) => return Ok(Some(lease)),
            LeaseAttempt::Held(Some(other)) if other.server_id == holder.server_id => {
                return Ok(None)
            }
            // The keeper of a replaced App Server leaves once that server is gone.
            LeaseAttempt::Held(other) => {
                if let Some(other) = other {
                    terminate(other.pid);
                }
                if Instant::now() >= deadline {
                    return Err("keeper_busy");
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
}

fn hold(client: &mut LocalClient, policy: KeeperPolicy) -> &'static str {
    let terminated = termination_flag();
    let mut clock = KeeperClock::new(policy, Instant::now());
    let mut last_heartbeat = Instant::now();
    let mut last_observation: Option<Instant> = None;
    let mut last_verdict: Option<Verdict> = None;
    let mut missed = 0;
    loop {
        std::thread::sleep(TICK);
        if terminated.load(Ordering::SeqCst) {
            return "stop_requested";
        }
        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
            last_heartbeat = Instant::now();
            match client.heartbeat() {
                Ok(_) => missed = 0,
                // The App Server forgot this client, as after the computer slept
                // past the liveness window.
                Err(ClientError::Refused { code, .. }) => {
                    logging::warn("keeper_rejoining", json!({ "error": code }));
                    if client.initialize().is_err() {
                        return "app_server_gone";
                    }
                }
                Err(error) => {
                    missed += 1;
                    logging::warn(
                        "keeper_heartbeat_missed",
                        json!({ "attempt": missed, "error": error.class() }),
                    );
                    if missed >= MAX_MISSED_HEARTBEATS {
                        return "app_server_gone";
                    }
                }
            }
        }
        if last_observation.is_some_and(|at| at.elapsed() < OBSERVE_INTERVAL) {
            continue;
        }
        last_observation = Some(Instant::now());
        let observation = match observe(client) {
            Ok(observation) => observation,
            // An App Server too old to say what it is doing cannot be judged, and
            // holding it forever would be worse than leaving.
            Err(ClientError::Refused { .. }) => return "impact_unsupported",
            Err(error) => {
                logging::warn("keeper_observe_failed", json!({ "error": error.class() }));
                continue;
            }
        };
        let verdict = clock.observe(&observation, Instant::now());
        if last_verdict != Some(verdict) {
            log_verdict(verdict, &observation);
            last_verdict = Some(verdict);
        }
        if let Verdict::Leave(reason) = verdict {
            return reason.class();
        }
    }
}

fn observe(client: &mut LocalClient) -> Result<Observation, ClientError> {
    let answer = client.request(CLIENT_REPLACEMENT_IMPACT, json!({}))?;
    let impact: ClientReplacementImpactResult =
        serde_json::from_value(answer.result).map_err(|_| ClientError::Malformed)?;
    Ok(observation_from(&impact))
}

/// Open Terminals are deliberately not observed: they never keep App Server alive (ADR-0071).
pub(crate) fn observation_from(impact: &ClientReplacementImpactResult) -> Observation {
    Observation {
        active_turns: impact.active_turns,
        pending_task_requests: impact.pending_task_requests,
        app_shells: impact
            .other_clients
            .iter()
            .filter(|client| !matches!(client.kind, ShellKind::Terminal | ShellKind::Keeper))
            .count(),
    }
}

fn log_verdict(verdict: Verdict, observation: &Observation) {
    let (verdict, reason) = match verdict {
        Verdict::Hold(reason) => ("hold", reason.class()),
        Verdict::Leave(reason) => ("leave", reason.class()),
    };
    logging::info(
        "keeper_verdict_changed",
        json!({
            "verdict": verdict,
            "reason": reason,
            "active_turns": observation.active_turns,
            "pending_task_requests": observation.pending_task_requests,
            "app_shells": observation.app_shells,
        }),
    );
}

fn stop(state_root: &StateRoot, runtime_root: &Path) -> i32 {
    let started = Instant::now();
    let path = lease::lease_path(runtime_root, state_root.fingerprint().as_str());
    let outcome = match lease::current_holder(&path) {
        Ok(None) => "not_running",
        Ok(Some(holder)) => {
            terminate(holder.pid);
            let deadline = Instant::now() + STOP_WAIT;
            loop {
                match lease::current_holder(&path) {
                    Ok(None) => break "stopped",
                    _ if Instant::now() >= deadline => break "still_running",
                    _ => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        }
        Err(_) => "lease_unreadable",
    };
    logging::info(
        "keeper_stop_completed",
        json!({ "outcome": outcome, "duration_ms": elapsed_ms(started) }),
    );
    println!("{}", json!({ "outcome": outcome }));
    i32::from(outcome != "stopped" && outcome != "not_running")
}

#[cfg(unix)]
fn terminate(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    if let Ok(pid) = i32::try_from(pid) {
        let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate(_pid: u32) {}

/// Only an explicit stop ends the keeper early. It has no terminal, so a hangup is
/// not a request to stop.
fn termination_flag() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        let _ = signal_hook::flag::register(signal, flag.clone());
    }
    #[cfg(unix)]
    let _ = signal_hook::flag::register(
        signal_hook::consts::SIGHUP,
        Arc::new(AtomicBool::new(false)),
    );
    flag
}

fn print_report(report: &Report) {
    // The reader is `keep start`, which exits after one line; a later write
    // failing must not end the keeper.
    let line = serde_json::to_string(report).unwrap_or_default();
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

fn mode_class(mode: Mode) -> &'static str {
    match mode {
        Mode::Own => "own",
        Mode::Replace => "replace",
        Mode::Join => "join",
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "keeper_command_tests.rs"]
mod tests;
