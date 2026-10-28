//! `openaide-app-server remote`: keeps this computer's App Server running for
//! Remote Devices from a terminal, and pairs, lists, and removes them there.
//!
//! The command is a same-machine client with no window (ADR-0064). The App
//! Server keeps its own rule: it stops after its last same-machine client
//! leaves, so it lives while this command runs and no longer.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openaide_app_server::logging;
use openaide_app_server::storage_runtime::StateRoot;
use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION;
use openaide_app_server_protocol::devices::{
    DeviceCollectionSnapshot, DevicesCreateInviteResult, RemoteAccessState,
};
use openaide_app_server_protocol::methods::{DEVICES_CREATE_INVITE, DEVICES_REMOVE};
use serde_json::json;

pub(crate) mod launch;
pub(crate) mod local_client;
mod text;

use launch::Connection;
use local_client::{ClientError, LocalClient};

/// Well inside the App Server's liveness window, and short enough that a
/// device connecting or leaving is printed while the user still watches.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const TICK: Duration = Duration::from_millis(100);
/// Consecutive unanswered heartbeats before the App Server is reported gone.
const MAX_MISSED_HEARTBEATS: u32 = 5;
const REMOTE_ACCESS_WAIT: Duration = Duration::from_secs(20);

const USAGE: &str = "\
Usage:
  openaide-app-server remote [--pair]       Keep this computer reachable by its paired devices until stopped
  openaide-app-server remote pair           Show a pairing code for a new device
  openaide-app-server remote devices        List the paired devices
  openaide-app-server remote remove <number> [--yes]
                                            Remove a paired device by its number in the list
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    Run { pair: bool },
    Pair,
    Devices,
    Remove { number: usize, confirmed: bool },
    Help,
}

/// Reads the arguments after `remote`. An unknown word is an error rather
/// than a guess, because one of these commands cuts a device off.
pub(crate) fn parse(arguments: &[String]) -> Result<Action, String> {
    let mut words = arguments.iter().map(String::as_str);
    match words.next() {
        None => Ok(Action::Run { pair: false }),
        Some("--pair") if words.next().is_none() => Ok(Action::Run { pair: true }),
        Some("pair") if words.next().is_none() => Ok(Action::Pair),
        Some("devices") if words.next().is_none() => Ok(Action::Devices),
        Some("help" | "--help" | "-h") => Ok(Action::Help),
        Some("remove") => {
            let mut number = None;
            let mut confirmed = false;
            for word in words {
                match word {
                    "--yes" | "-y" => confirmed = true,
                    word if number.is_none() => {
                        number = word.parse::<usize>().ok().filter(|number| *number > 0);
                        if number.is_none() {
                            return Err(format!("\"{word}\" is not a device number."));
                        }
                    }
                    word => return Err(format!("Unexpected \"{word}\".")),
                }
            }
            number
                .map(|number| Action::Remove { number, confirmed })
                .ok_or_else(|| "Give the number of the device to remove.".to_string())
        }
        Some(word) => Err(format!("Unknown command \"{word}\".")),
    }
}

/// Runs the command and returns the process exit code.
pub(crate) fn run(arguments: &[String], state_root: &StateRoot, runtime_root: &Path) -> i32 {
    let action = match parse(arguments) {
        Ok(action) => action,
        Err(problem) => {
            eprintln!("{problem}\n\n{USAGE}");
            return 2;
        }
    };
    let operation = match &action {
        Action::Run { .. } => "run",
        Action::Pair => "pair",
        Action::Devices => "devices",
        Action::Remove { .. } => "remove",
        Action::Help => {
            print!("{USAGE}");
            return 0;
        }
    };
    let started = Instant::now();
    logging::info("remote_command_started", json!({ "operation": operation }));
    let outcome = match action {
        Action::Run { pair } => keep_running(state_root, runtime_root, pair),
        Action::Pair => with_running(state_root, runtime_root, show_pairing_code),
        Action::Devices => with_running(state_root, runtime_root, |client| {
            print!("{}", text::device_list(&client.devices()?));
            Ok(())
        }),
        Action::Remove { number, confirmed } => with_running(state_root, runtime_root, |client| {
            remove_device(client, number, confirmed)
        }),
        Action::Help => unreachable!("help returned above"),
    };
    let (code, outcome_name) = match &outcome {
        Ok(()) => (0, "success"),
        Err(failure) => {
            eprintln!("{}", failure.message());
            (1, failure.class())
        }
    };
    logging::info(
        "remote_command_completed",
        json!({
            "operation": operation,
            "outcome": outcome_name,
            "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        }),
    );
    code
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Failure {
    NotRunning,
    /// An App Server of another version runs for this state root and is left running.
    OtherVersion {
        protocol_version: String,
    },
    /// The running App Server could not be looked up; the class says why.
    Handoff(&'static str),
    Launch(launch::LaunchError),
    Client(ClientError),
    RemoteAccess,
    NoSuchDevice(usize),
    Cancelled,
    Stopped,
}

impl From<ClientError> for Failure {
    fn from(error: ClientError) -> Self {
        Self::Client(error)
    }
}

impl Failure {
    fn class(&self) -> &'static str {
        match self {
            Self::NotRunning => "not_running",
            Self::OtherVersion { .. } => "other_version_running",
            Self::Handoff(class) => class,
            Self::Launch(error) => error.class(),
            Self::Client(error) => error.class(),
            Self::RemoteAccess => "remote_access_failed",
            Self::NoSuchDevice(_) => "no_such_device",
            Self::Cancelled => "cancelled",
            Self::Stopped => "app_server_stopped",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::NotRunning => "OpenAIDE is not running on this computer. Start it with: openaide-app-server remote".to_string(),
            Self::OtherVersion { .. } => "Another version of OpenAIDE is running on this computer and was left running. Use the openaide-app-server that started it.".to_string(),
            Self::Handoff(_) => "OpenAIDE on this computer could not be reached. Its log has the reason.".to_string(),
            Self::Launch(_) => "OpenAIDE could not start. Its log has the reason.".to_string(),
            Self::Client(ClientError::Refused { message, .. }) if !message.is_empty() => {
                format!("OpenAIDE refused the request: {message}")
            }
            Self::Client(_) => "OpenAIDE on this computer did not answer.".to_string(),
            Self::RemoteAccess => "This computer could not open its connection for devices. Check its network and try again.".to_string(),
            Self::NoSuchDevice(number) => format!("There is no device number {number}. See the list with: openaide-app-server remote devices"),
            Self::Cancelled => "Nothing was removed.".to_string(),
            Self::Stopped => "OpenAIDE stopped unexpectedly. Its log has the reason.".to_string(),
        }
    }
}

fn connect(connection: &Connection) -> Result<LocalClient, Failure> {
    let mut client = LocalClient::new(&connection.endpoint_url, &connection.auth_token)?;
    client.initialize()?;
    Ok(client)
}

/// Serves one request against an App Server something else keeps running.
fn with_running(
    state_root: &StateRoot,
    runtime_root: &Path,
    work: impl FnOnce(&mut LocalClient) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let connection = match launch::running(state_root, runtime_root).map_err(Failure::Handoff)? {
        launch::Found::Joined(connection) => connection,
        launch::Found::NotRunning => return Err(Failure::NotRunning),
        launch::Found::OtherVersion { protocol_version } => {
            logging::warn(
                "remote_command_other_version_running",
                json!({
                    "running_protocol_version": protocol_version,
                    "required_protocol_version": APP_SERVER_PROTOCOL_VERSION,
                }),
            );
            return Err(Failure::OtherVersion { protocol_version });
        }
    };
    let mut client = connect(&connection)?;
    let outcome = work(&mut client);
    // Leaving at once keeps a short command from holding the App Server
    // through its liveness window.
    let _ = client.detach();
    outcome
}

fn keep_running(state_root: &StateRoot, runtime_root: &Path, pair: bool) -> Result<(), Failure> {
    let interrupts = Arc::new(AtomicU32::new(0));
    {
        let interrupts = interrupts.clone();
        // Without the handler the first Ctrl+C would end the command before
        // it could say what stops with it.
        let _ = ctrlc::set_handler(move || {
            interrupts.fetch_add(1, Ordering::SeqCst);
        });
    }
    let terminated = termination_flag();
    let connection = launch::attach_or_start(state_root, runtime_root).map_err(Failure::Launch)?;
    let mut client = connect(&connection)?;
    logging::info(
        "remote_command_attached",
        json!({ "started_app_server": connection.started }),
    );
    let mut devices = client.devices()?;
    println!(
        "OpenAIDE is running on {}. Paired devices can reach it while this command runs.",
        devices.server_name
    );
    if pair {
        show_pairing_code(&mut client)?;
        devices = client.devices()?;
    } else if devices.devices.is_empty() {
        println!(
            "No devices are paired yet. In another terminal, run: openaide-app-server remote pair"
        );
    } else {
        println!(
            "To pair another device, run in another terminal: openaide-app-server remote pair"
        );
    }
    println!("Press Ctrl+C to stop.");

    let mut last_heartbeat = Instant::now();
    let mut missed = 0;
    let mut warned = false;
    loop {
        std::thread::sleep(TICK);
        // A service manager or a closed terminal cannot answer a warning,
        // so its stop is final at once.
        if terminated.load(Ordering::SeqCst) {
            logging::info(
                "remote_command_stop_requested",
                json!({ "by": "termination" }),
            );
            let _ = client.detach();
            // The terminal may already be gone; a failed write must not
            // turn a clean stop into a crash.
            let _ = writeln!(std::io::stdout(), "Stopped.");
            return Ok(());
        }
        let pressed = interrupts.load(Ordering::SeqCst);
        if pressed >= 2 || (pressed == 1 && warned) {
            break;
        }
        if pressed == 1 {
            match client.running_task_count() {
                Ok(count) if count > 0 => {
                    println!("{}", text::stop_warning(count));
                    warned = true;
                    interrupts.store(0, Ordering::SeqCst);
                    continue;
                }
                _ => break,
            }
        }
        if last_heartbeat.elapsed() < HEARTBEAT_INTERVAL {
            continue;
        }
        last_heartbeat = Instant::now();
        match client.heartbeat() {
            Ok(update) => {
                missed = 0;
                if let Some(update) = update {
                    report_changes(&devices, &update);
                    devices = update;
                }
            }
            // The App Server forgot this client, as after the computer slept
            // past the liveness window. It is the same App Server, so joining
            // again is all there is to recover.
            Err(ClientError::Refused { code, .. }) => {
                logging::warn("remote_command_rejoining", json!({ "error": code }));
                client.initialize()?;
                let update = client.devices()?;
                report_changes(&devices, &update);
                devices = update;
            }
            Err(error) => {
                missed += 1;
                logging::warn(
                    "remote_command_heartbeat_missed",
                    json!({ "attempt": missed, "error": error.class() }),
                );
                if missed >= MAX_MISSED_HEARTBEATS {
                    return Err(Failure::Stopped);
                }
            }
        }
    }
    logging::info(
        "remote_command_stop_requested",
        json!({ "by": "interrupt" }),
    );
    let _ = client.detach();
    println!("Stopped.");
    Ok(())
}

/// Set when the command is told to end without a keyboard: a service stop or
/// its terminal closing. Other systems end the process outright.
fn termination_flag() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        let _ = signal_hook::flag::register(signal, flag.clone());
    }
    flag
}

fn report_changes(before: &DeviceCollectionSnapshot, after: &DeviceCollectionSnapshot) {
    let time = chrono::Local::now().format("%H:%M");
    for change in text::changes(before, after) {
        logging::info(
            "remote_command_device_changed",
            json!({ "change": change.class() }),
        );
        println!("{time}  {}", change.line());
    }
}

fn show_pairing_code(client: &mut LocalClient) -> Result<(), Failure> {
    let answer = client.request(DEVICES_CREATE_INVITE, json!({}))?;
    let invite: DevicesCreateInviteResult =
        serde_json::from_value(answer.result).map_err(|_| ClientError::Malformed)?;
    // The first code is what opens this computer to devices, so the code is
    // shown only once a device could actually use it.
    let waiting = Instant::now();
    loop {
        match client.devices()?.remote_access {
            RemoteAccessState::On => break,
            RemoteAccessState::Failed => return Err(Failure::RemoteAccess),
            _ if waiting.elapsed() > REMOTE_ACCESS_WAIT => return Err(Failure::RemoteAccess),
            _ => std::thread::sleep(Duration::from_millis(250)),
        }
    }
    let now_ms = u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0);
    println!();
    if std::io::stdout().is_terminal() {
        if let Some(picture) = qr_picture(&invite.code) {
            println!("{picture}");
        }
    }
    println!("Pairing code (paste it in the app on the other device):\n");
    println!("{}\n", invite.code);
    println!(
        "It works once and for {} minutes. Anyone who has it can use this computer.",
        text::minutes_left(invite.expires_at_ms, now_ms)
    );
    Ok(())
}

fn qr_picture(code: &str) -> Option<String> {
    let picture = qrcode::QrCode::new(code.as_bytes()).ok()?;
    use qrcode::render::unicode::Dense1x2;
    // A terminal usually draws light glyphs on a dark background, so the
    // code's dark modules are the cells left empty.
    Some(
        picture
            .render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
    )
}

fn remove_device(client: &mut LocalClient, number: usize, confirmed: bool) -> Result<(), Failure> {
    let snapshot = client.devices()?;
    let devices = text::ordered(&snapshot);
    let device = devices
        .get(number - 1)
        .ok_or(Failure::NoSuchDevice(number))?;
    let name = text::label(device);
    if !confirmed {
        print!("Remove {name}? It will have to be paired again. (y/n) ");
        let _ = std::io::stdout().flush();
        let mut answer = String::new();
        let _ = std::io::stdin().lock().read_line(&mut answer);
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            return Err(Failure::Cancelled);
        }
    }
    client.request(DEVICES_REMOVE, json!({ "deviceId": device.device_id }))?;
    println!("Removed {name}.");
    Ok(())
}

#[cfg(test)]
#[path = "remote_command_tests.rs"]
mod tests;
