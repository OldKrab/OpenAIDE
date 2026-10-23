//! Terminals: the user's own shells on the App Server's computer (ADR-0071).
//!
//! App Server owns every shell. A Terminal outlives the clients viewing it and ends when closed,
//! when its shell exits and the user closes it, or when App Server stops. Nothing here is
//! persisted, and Agents never see a Terminal.
//!
//! Lifecycle requests come through the protocol gateway, which publishes the list itself. Status
//! changes a shell reports on its own reach clients through `on_changed`, which never runs while
//! a gateway call into this registry is in progress.

mod replay;
mod shell;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::terminals::{
    TerminalAttach, TerminalCollectionSnapshot, TerminalStatus, TerminalSummary,
};
use serde_json::json;

pub(crate) use shell::{valid_size, Shell};

/// Path suffix the local HTTP listener routes to a Terminal socket.
pub const TERMINAL_SOCKET_PATH: &str = "/terminal";
/// Long enough for a client to open the socket after the response, short enough that a leaked
/// token is useless soon after.
const ATTACH_TOKEN_TTL: Duration = Duration::from_secs(30);

type ChangedCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Default)]
pub struct Terminals {
    inner: Arc<Mutex<Inner>>,
    changed: Arc<Mutex<Option<ChangedCallback>>>,
}

#[derive(Default)]
struct Inner {
    /// Ordered by opening, so every client lists tabs the same way.
    shells: Vec<Arc<Shell>>,
    attach_tokens: HashMap<String, PendingAttach>,
}

struct PendingAttach {
    terminal_id: String,
    expires_at: Instant,
}

/// What the gateway publishes after a lifecycle change.
pub(crate) struct Opened {
    pub(crate) terminal: TerminalSummary,
    pub(crate) attach: TerminalAttach,
}

impl Terminals {
    /// Runs on a shell's monitor thread whenever a Terminal's published status changes.
    pub fn on_changed(&self, changed: impl Fn() + Send + Sync + 'static) {
        *self.changed.lock().expect("terminal callback poisoned") = Some(Arc::new(changed));
    }

    pub fn snapshot(&self) -> TerminalCollectionSnapshot {
        let shells = self
            .inner
            .lock()
            .expect("terminals poisoned")
            .shells
            .clone();
        TerminalCollectionSnapshot {
            terminals: shells.iter().map(|shell| shell.summary()).collect(),
        }
    }

    /// Running Terminals, which stopping App Server would end.
    pub fn open_count(&self) -> usize {
        self.inner
            .lock()
            .expect("terminals poisoned")
            .shells
            .iter()
            .filter(|shell| shell.summary().status != TerminalStatus::Exited)
            .count()
    }

    pub(crate) fn open(
        &self,
        project_id: ProjectId,
        folder: &Path,
        cols: u16,
        rows: u16,
        target_kind: &'static str,
    ) -> Result<Opened, ProtocolError> {
        if !valid_size(cols, rows) {
            return Err(error(
                ProtocolErrorCode::ValidationFailed,
                "Terminal size is out of range",
            ));
        }
        let started = Instant::now();
        let terminal_id = format!("terminal_{}", uuid::Uuid::new_v4().simple());
        crate::logging::info(
            "terminal_open_started",
            json!({ "terminal_id": terminal_id, "target_kind": target_kind, "attempt": 1 }),
        );
        let changed = self.changed.clone();
        let spawned = Shell::spawn(
            terminal_id.clone(),
            project_id,
            folder,
            cols,
            rows,
            unix_millis(),
            move |shell| {
                log_status_change(shell);
                let callback = changed.lock().expect("terminal callback poisoned").clone();
                if let Some(callback) = callback {
                    callback();
                }
            },
        );
        let shell = match spawned {
            Ok(shell) => shell,
            Err(spawn_error) => {
                crate::logging::warn(
                    "terminal_open_completed",
                    json!({
                        "terminal_id": terminal_id,
                        "target_kind": target_kind,
                        "outcome": "failure",
                        "error_kind": format!("{:?}", spawn_error.kind()),
                        "duration_ms": started.elapsed().as_millis(),
                        "attempt": 1,
                    }),
                );
                return Err(error(
                    ProtocolErrorCode::Internal,
                    "The shell could not be started",
                ));
            }
        };
        let terminal = shell.summary();
        let mut inner = self.inner.lock().expect("terminals poisoned");
        inner.shells.push(shell);
        let attach = inner.issue_attach(&terminal_id);
        let open = inner.shells.len();
        drop(inner);
        crate::logging::info(
            "terminal_open_completed",
            json!({
                "terminal_id": terminal_id,
                "target_kind": target_kind,
                "outcome": "success",
                "terminals_open": open,
                "duration_ms": started.elapsed().as_millis(),
                "attempt": 1,
            }),
        );
        Ok(Opened { terminal, attach })
    }

    pub(crate) fn attach(&self, terminal_id: &str) -> Result<TerminalAttach, ProtocolError> {
        let mut inner = self.inner.lock().expect("terminals poisoned");
        let Some(shell) = inner.shell(terminal_id) else {
            crate::logging::warn(
                "terminal_attach_issued",
                json!({ "terminal_id": terminal_id, "outcome": "not_found" }),
            );
            return Err(not_found());
        };
        let attach = inner.issue_attach(terminal_id);
        drop(inner);
        // Pairs with `terminal_socket_opened` or `terminal_socket_rejected` for the same id.
        crate::logging::info(
            "terminal_attach_issued",
            json!({
                "terminal_id": terminal_id,
                "outcome": "success",
                "viewers": shell.viewer_count(),
            }),
        );
        Ok(attach)
    }

    /// Fails with `conflict` when a command is running and the user has not confirmed.
    pub(crate) fn close(&self, terminal_id: &str, force: bool) -> Result<(), ProtocolError> {
        let mut inner = self.inner.lock().expect("terminals poisoned");
        let Some(shell) = inner.shell(terminal_id) else {
            return Err(not_found());
        };
        let busy = shell.is_busy();
        if busy && !force {
            return Err(error(
                ProtocolErrorCode::Conflict,
                "A command is running in this Terminal",
            ));
        }
        inner
            .shells
            .retain(|shell| shell.terminal_id != terminal_id);
        inner
            .attach_tokens
            .retain(|_, pending| pending.terminal_id != terminal_id);
        let open = inner.shells.len();
        drop(inner);
        let status = shell.summary().status;
        let viewers = shell.viewer_count();
        shell.terminate();
        crate::logging::info(
            "terminal_closed",
            json!({
                "terminal_id": terminal_id,
                "status": status,
                "forced": busy,
                "viewers": viewers,
                "terminals_open": open,
            }),
        );
        Ok(())
    }

    /// Ends every shell, for an App Server stop. A process exit also hangs up each PTY, which
    /// ends the shells, but closing here reaps them and records the closes.
    pub fn close_all(&self) {
        let shells = std::mem::take(&mut self.inner.lock().expect("terminals poisoned").shells);
        if shells.is_empty() {
            return;
        }
        for shell in &shells {
            shell.terminate();
        }
        crate::logging::info("terminals_closed_all", json!({ "terminals": shells.len() }));
    }

    /// Redeems a single-use attach token for the socket that presents it.
    pub(crate) fn claim(&self, token: &str) -> Option<Arc<Shell>> {
        let mut inner = self.inner.lock().expect("terminals poisoned");
        let now = Instant::now();
        inner
            .attach_tokens
            .retain(|_, pending| pending.expires_at > now);
        let pending = inner.attach_tokens.remove(token)?;
        inner.shell(&pending.terminal_id)
    }
}

impl Inner {
    fn shell(&self, terminal_id: &str) -> Option<Arc<Shell>> {
        self.shells
            .iter()
            .find(|shell| shell.terminal_id == terminal_id)
            .cloned()
    }

    fn issue_attach(&mut self, terminal_id: &str) -> TerminalAttach {
        let now = Instant::now();
        self.attach_tokens
            .retain(|_, pending| pending.expires_at > now);
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.attach_tokens.insert(
            token.clone(),
            PendingAttach {
                terminal_id: terminal_id.to_string(),
                expires_at: now + ATTACH_TOKEN_TTL,
            },
        );
        TerminalAttach {
            terminal_id: terminal_id.to_string(),
            token,
            path: TERMINAL_SOCKET_PATH.to_string(),
        }
    }
}

fn log_status_change(shell: &Shell) {
    let summary = shell.summary();
    let event = if summary.status == TerminalStatus::Exited {
        "terminal_shell_exited"
    } else {
        "terminal_status_changed"
    };
    // The command name and folder are user content and stay out of logs.
    crate::logging::info(
        event,
        json!({
            "terminal_id": summary.terminal_id,
            "status": summary.status,
            "exit_code": summary.exit_code,
        }),
    );
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

fn not_found() -> ProtocolError {
    error(ProtocolErrorCode::NotFound, "Terminal is not open")
}

fn error(code: ProtocolErrorCode, message: &str) -> ProtocolError {
    ProtocolError {
        code,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}

#[cfg(all(test, unix))]
#[path = "terminals_tests.rs"]
mod tests;
