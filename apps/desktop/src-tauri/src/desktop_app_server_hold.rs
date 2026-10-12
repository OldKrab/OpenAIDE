//! Keeps Desktop a client of the App Server it started for as long as the Desktop
//! process runs.
//!
//! The window's own client heartbeats from a WebView timer, and the platform may
//! stall those timers while the window is hidden. The App Server then saw its
//! last client expire and shut down under work in progress. This client belongs
//! to the native process instead, so the App Server's lifetime follows Desktop's
//! and not the window's timers. It subscribes to nothing and answers nothing.
//!
//! Quit and Desktop Update end the hold explicitly, after the window's client has
//! detached, so the App Server still receives its last-client signal at once. A
//! Desktop that dies without that stops heartbeating and expires like any client.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// Loopback answers at once; a longer wait only delays noticing a replaced App Server.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

/// The App Server generation to hold: where it listens and the token it accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HoldTarget {
    pub(crate) endpoint_url: String,
    pub(crate) auth_token: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HoldError {
    /// Nothing answered at the published address.
    Unreachable,
    /// The App Server answered and refused, as it does for a client it no longer knows.
    Refused,
    /// The endpoint or the answer could not be read as the protocol.
    Malformed,
}

impl HoldError {
    /// Stable, content-free name for diagnostics.
    fn class(&self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::Refused => "refused",
            Self::Malformed => "malformed",
        }
    }
}

/// Starts the hold for the App Server this Desktop owns, following every replacement.
pub(crate) fn install(app: &tauri::App) {
    use tauri::Manager;
    let hold = AppServerHold::new();
    app.manage(hold.clone());
    let app = app.handle().clone();
    hold.start(move || {
        let desktop = app.state::<crate::DesktopState>();
        // A paired computer's App Server is kept running by a shell on that computer.
        if !desktop.owns_app_server() {
            return None;
        }
        let connection = desktop.connection.lock().ok()?.clone()?;
        Some(HoldTarget {
            endpoint_url: connection.endpoint_url,
            auth_token: connection.auth_token,
        })
    });
}

/// The last step of Quit. The window's client has detached by now, so ending the
/// hold is the App Server's last-client signal.
#[tauri::command]
pub(crate) async fn complete_desktop_quit(app: tauri::AppHandle) {
    use tauri::Manager;
    let hold = app.state::<AppServerHold>().inner().clone();
    let _ = tauri::async_runtime::spawn_blocking(move || hold.end("quit")).await;
    app.exit(0);
}

#[derive(Default)]
struct HoldState {
    held: Option<HoldTarget>,
    ended: bool,
    /// Consecutive failed heartbeats; only the first and the recovery are logged.
    failures: u32,
    next_request: u64,
}

#[derive(Clone)]
pub(crate) struct AppServerHold {
    client_instance_id: String,
    state: Arc<Mutex<HoldState>>,
}

impl AppServerHold {
    pub(crate) fn new() -> Self {
        Self {
            client_instance_id: format!("desktop-native-{}", uuid::Uuid::new_v4()),
            state: Arc::new(Mutex::new(HoldState::default())),
        }
    }

    /// Runs the hold until the process exits. `target` names the App Server this
    /// Desktop currently owns, or `None` while it owns none.
    pub(crate) fn start(&self, target: impl Fn() -> Option<HoldTarget> + Send + 'static) {
        let hold = self.clone();
        std::thread::spawn(move || {
            loop {
                hold.sync(target());
                std::thread::sleep(HEARTBEAT_INTERVAL);
            }
        });
    }

    /// One step: follow `target` to a new App Server generation, or renew the held one.
    pub(crate) fn sync(&self, target: Option<HoldTarget>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.ended {
            return;
        }
        if state.held != target {
            if let Some(previous) = state.held.take() {
                // A replaced App Server is gone and refuses nothing; one Desktop no
                // longer owns (a paired computer is in use) is told so it may stop.
                let _ = self.request(&mut state, &previous, "client/detach", json!({}));
            }
            state.failures = 0;
            let Some(target) = target else {
                eprintln!("desktop_app_server_hold_ended reason_code=not_owned");
                return;
            };
            let started_at = Instant::now();
            match self.initialize(&mut state, &target) {
                Ok(()) => {
                    eprintln!(
                        "desktop_app_server_hold_started outcome=success duration_ms={}",
                        started_at.elapsed().as_millis()
                    );
                    state.held = Some(target);
                }
                // The next step tries again; the interval is the retry.
                Err(error) => eprintln!(
                    "desktop_app_server_hold_started outcome=failure error_kind={} duration_ms={}",
                    error.class(),
                    started_at.elapsed().as_millis()
                ),
            }
            return;
        }
        let Some(target) = target else {
            return;
        };
        match self.request(&mut state, &target, "client/heartbeat", json!({})) {
            Ok(()) => {
                if state.failures > 0 {
                    eprintln!(
                        "desktop_app_server_hold_heartbeat_recovered attempt_count={}",
                        state.failures
                    );
                }
                state.failures = 0;
            }
            Err(error) => {
                state.failures += 1;
                if state.failures == 1 {
                    eprintln!(
                        "desktop_app_server_hold_heartbeat_failed error_kind={}",
                        error.class()
                    );
                }
                // An App Server that forgot this client is asked again from the start.
                if error == HoldError::Refused {
                    state.held = None;
                }
            }
        }
    }

    /// Ends the hold for good and tells the App Server now, so a Desktop that is
    /// quitting or updating is not kept waiting for this client to expire.
    pub(crate) fn end(&self, reason_code: &'static str) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.ended {
            return;
        }
        state.ended = true;
        let Some(target) = state.held.take() else {
            eprintln!("desktop_app_server_hold_ended reason_code={reason_code} outcome=not_held");
            return;
        };
        let started_at = Instant::now();
        let outcome = match self.request(&mut state, &target, "client/detach", json!({})) {
            Ok(()) => "detached",
            // The App Server may exit on its final detach before the answer is readable.
            Err(HoldError::Unreachable) => "unreachable",
            Err(HoldError::Refused) => "refused",
            Err(HoldError::Malformed) => "malformed",
        };
        eprintln!(
            "desktop_app_server_hold_ended reason_code={reason_code} outcome={outcome} duration_ms={}",
            started_at.elapsed().as_millis()
        );
    }

    fn initialize(&self, state: &mut HoldState, target: &HoldTarget) -> Result<(), HoldError> {
        self.request(
            state,
            target,
            "client/initialize",
            json!({
                "clientInstanceId": self.client_instance_id,
                "shell": {
                    "kind": "desktop",
                    "name": "OpenAIDE",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "requestedSurface": { "kind": "home" },
                "workspaceRoots": [],
            }),
        )
    }

    /// One plain HTTP request to the loopback endpoint, as a same-machine client with no window.
    fn request(
        &self,
        state: &mut HoldState,
        target: &HoldTarget,
        method: &str,
        params: Value,
    ) -> Result<(), HoldError> {
        let (address, path) =
            loopback_endpoint(&target.endpoint_url).ok_or(HoldError::Malformed)?;
        state.next_request += 1;
        let id = format!("desktop-native-{}", state.next_request);
        let body =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        let wire = format!(
            "POST {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nX-OpenAIDE-Connection-Id: {}\r\nX-OpenAIDE-Client-Instance-Id: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            target.auth_token,
            self.client_instance_id,
            self.client_instance_id,
            body.len(),
        );
        let mut stream = TcpStream::connect_timeout(&address, REQUEST_TIMEOUT)
            .map_err(|_| HoldError::Unreachable)?;
        stream
            .set_read_timeout(Some(REQUEST_TIMEOUT))
            .and_then(|()| stream.set_write_timeout(Some(REQUEST_TIMEOUT)))
            .map_err(|_| HoldError::Unreachable)?;
        stream
            .write_all(wire.as_bytes())
            .map_err(|_| HoldError::Unreachable)?;
        let mut response = Vec::new();
        stream
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut response)
            .map_err(|_| HoldError::Unreachable)?;
        read_reply(&response, &id)
    }
}

/// The App Server's published address. Any host but loopback is refused: the token must not leave this machine.
fn loopback_endpoint(endpoint_url: &str) -> Option<(SocketAddr, String)> {
    let rest = endpoint_url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    let address: SocketAddr = authority.parse().ok()?;
    address
        .ip()
        .is_loopback()
        .then(|| (address, path.to_string()))
}

/// The App Server answers with one message or a list: the reply, then whatever was queued for this client.
fn read_reply(response: &[u8], id: &str) -> Result<(), HoldError> {
    let text = std::str::from_utf8(response).map_err(|_| HoldError::Malformed)?;
    let (head, body) = text.split_once("\r\n\r\n").ok_or(HoldError::Unreachable)?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or(HoldError::Malformed)?;
    if status == 401 || status == 403 {
        return Err(HoldError::Refused);
    }
    let value: Value = serde_json::from_str(body).map_err(|_| HoldError::Malformed)?;
    let messages = match value {
        Value::Array(messages) => messages,
        message => vec![message],
    };
    let reply = messages
        .iter()
        .find(|message| message["id"] == id)
        .ok_or(HoldError::Malformed)?;
    if reply.get("error").is_some() {
        return Err(HoldError::Refused);
    }
    Ok(())
}

#[cfg(test)]
#[path = "desktop_app_server_hold_tests.rs"]
mod tests;
