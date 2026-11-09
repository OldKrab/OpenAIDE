//! Interactive ACP authentication, scoped to the client which started sign-in.
//! Only trusted Agent configuration supplies the executable; terminal bytes are transient.
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine};
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::server_requests::{
    ShellAuthTerminalParams, ShellAuthTerminalResponse, SHELL_AUTH_TERMINAL,
};
use portable_pty::{native_pty_system, CommandBuilder};

use super::{acp_agent_config::AcpAgentConfig, TurnCancellation};
use crate::client_lifecycle::{AppServerTime, Delivery};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::pty_child::{pty_size as size, PtyChild};
use crate::server_requests::ServerRequestRuntime;

pub trait AuthTerminalRunner: Send + Sync {
    fn run(
        &self,
        config: AcpAgentConfig,
        cancellation: TurnCancellation,
    ) -> Result<(), RuntimeError>;
}

pub(super) struct CancelOnDrop {
    pub token: TurnCancellation,
}

impl CancelOnDrop {
    pub fn new() -> Self {
        Self {
            token: TurnCancellation::new(),
        }
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

pub(crate) struct ClientAuthTerminal {
    requests: ServerRequestRuntime,
    client_id: ClientInstanceId,
    delivery: Delivery,
}

impl ClientAuthTerminal {
    pub(crate) fn new(
        requests: ServerRequestRuntime,
        client_id: ClientInstanceId,
        delivery: Delivery,
    ) -> Self {
        Self {
            requests,
            client_id,
            delivery,
        }
    }

    fn exchange(
        &self,
        params: ShellAuthTerminalParams,
        cancellation: &TurnCancellation,
    ) -> Result<ShellAuthTerminalResponse, RuntimeError> {
        let opened = self.requests.open_waitable_client_request(
            self.client_id.clone(),
            self.delivery.clone(),
            SHELL_AUTH_TERMINAL,
            "Agent sign-in terminal",
            serde_json::to_value(params).map_err(|_| terminal_error())?,
            AppServerTime::now(),
        )?;
        // This is a transport liveness deadline, not a deadline for completing sign-in.
        // Each response acknowledges one output batch and carries at most one input batch.
        let result = self.requests.wait_client_response_cancellable(
            &opened.request_id,
            Duration::from_secs(30),
            cancellation,
        );
        self.requests
            .forget_auth_terminal_exchange(&opened.request_id);
        serde_json::from_value(result?).map_err(|_| terminal_error())
    }

    fn run_inner(
        &self,
        config: AcpAgentConfig,
        cancellation: &TurnCancellation,
        terminal_id: &str,
    ) -> Result<u32, TerminalEnd> {
        let pair = native_pty_system()
            .openpty(size(80, 24))
            .map_err(|_| TerminalEnd::new("pty_open_failed"))?;
        let mut command = CommandBuilder::new(&config.command);
        command.args(&config.args);
        command.env("TERM", "xterm-256color");
        for (key, value) in &config.env {
            command.env(key, value);
        }
        // Cancellation, disconnect, malformed responses and setup failures drop this child, which
        // never leaves a credential-taking process waiting unattended.
        let mut child = PtyChild::new(
            pair.slave
                .spawn_command(command)
                .map_err(|_| TerminalEnd::new("spawn_failed"))?,
        );
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|_| TerminalEnd::new("pty_open_failed"))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|_| TerminalEnd::new("pty_open_failed"))?;
        // PTY writes can block when an Agent stops reading input. Keep those writes off the
        // lifecycle worker so cancellation can always kill the owned process and unblock I/O.
        // The queue holds several batches so a paste arriving while a write is still in flight
        // waits instead of ending sign-in; at most 32 KiB per batch bounds its memory.
        let (input_send, input_receive) = mpsc::sync_channel::<Vec<u8>>(INPUT_QUEUE_BATCHES);
        std::thread::spawn(move || {
            while let Ok(bytes) = input_receive.recv() {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        // Backpressure bounds memory even if the login process prints continuously while the
        // browser is disconnected. Closing the receiver releases a blocked reader on teardown.
        let (send, receive) = mpsc::sync_channel::<Vec<u8>>(8);
        std::thread::spawn(move || {
            let mut bytes = [0_u8; 4096];
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 || send.send(bytes[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut dimensions = (80, 24);
        loop {
            if cancellation.is_cancelled() {
                return Err(TerminalEnd::new("cancelled"));
            }
            // Once exited, the child is never signalled again, even after awaiting another
            // browser exchange: its PID has been reaped and the OS may reuse it.
            let exited = child
                .try_exit()
                .map_err(|_| TerminalEnd::new("wait_failed"))?;
            let mut output = Vec::new();
            while output.len() < 32768 {
                let Ok(bytes) = receive.try_recv() else { break };
                output.extend(bytes);
            }
            let response = self
                .exchange(
                    ShellAuthTerminalParams {
                        agent_id: config.agent_id.clone(),
                        terminal_id: terminal_id.to_string(),
                        output: STANDARD.encode(output),
                        exited: exited.is_some(),
                    },
                    cancellation,
                )
                .map_err(|error| TerminalEnd::from_error("client_exchange_failed", error))?;
            if response.cancel {
                return Err(TerminalEnd::new("client_cancelled"));
            }
            if let Some(status) = exited {
                return if status.success() {
                    Ok(status.exit_code())
                } else {
                    Err(TerminalEnd {
                        exit_code: Some(status.exit_code()),
                        ..TerminalEnd::new("login_exited_unsuccessfully")
                    })
                };
            }
            if response.input.len() > 32768
                || !(2..=500).contains(&response.cols)
                || !(2..=200).contains(&response.rows)
            {
                return Err(TerminalEnd::from_error(
                    "invalid_client_response",
                    RuntimeError::InvalidParams("authentication terminal input".into()),
                ));
            }
            if dimensions != (response.cols, response.rows) {
                pair.master
                    .resize(size(response.cols, response.rows))
                    .map_err(|_| TerminalEnd::new("resize_failed"))?;
                dimensions = (response.cols, response.rows);
            }
            let input = STANDARD
                .decode(response.input)
                .map_err(|_| TerminalEnd::new("invalid_client_response"))?;
            if !input.is_empty() {
                input_send.try_send(input).map_err(|error| {
                    TerminalEnd::new(match error {
                        mpsc::TrySendError::Full(_) => "input_backlog_full",
                        mpsc::TrySendError::Disconnected(_) => "input_closed",
                    })
                })?;
            }
        }
    }
}

/// Input batches waiting for the PTY writer; each is at most 32 KiB.
const INPUT_QUEUE_BATCHES: usize = 64;

/// Why a sign-in terminal ended without signing in. The reason and exit code are diagnostics
/// only; the user sees one sign-in failure for all of them.
struct TerminalEnd {
    reason: &'static str,
    exit_code: Option<u32>,
    error: RuntimeError,
}

impl TerminalEnd {
    fn new(reason: &'static str) -> Self {
        Self::from_error(reason, terminal_error())
    }

    fn from_error(reason: &'static str, error: RuntimeError) -> Self {
        Self {
            reason,
            exit_code: None,
            error,
        }
    }
}

impl AuthTerminalRunner for ClientAuthTerminal {
    fn run(
        &self,
        config: AcpAgentConfig,
        cancellation: TurnCancellation,
    ) -> Result<(), RuntimeError> {
        let terminal_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        logging::info(
            "agent_auth_terminal_started",
            serde_json::json!({
                "operation_id": terminal_id, "agent_id": config.agent_id, "attempt": 1,
            }),
        );
        let result = self.run_inner(config, &cancellation, &terminal_id);
        let (end_reason, exit_code, error_kind) = match &result {
            Ok(code) => ("login_exited", Some(*code), None),
            Err(end) => (end.reason, end.exit_code, Some(end.error.reason())),
        };
        logging::info(
            "agent_auth_terminal_completed",
            serde_json::json!({
                "operation_id": terminal_id, "duration_ms": started.elapsed().as_millis(),
                "outcome": if cancellation.is_cancelled() { "cancelled" } else if result.is_ok() { "success" } else { "failure" },
                "end_reason": end_reason, "exit_code": exit_code,
                "error_kind": error_kind, "attempt": 1,
            }),
        );
        result.map(|_| ()).map_err(|end| end.error)
    }
}

fn terminal_error() -> RuntimeError {
    RuntimeError::NotReady("Agent authentication terminal ended without successful sign-in".into())
}

#[cfg(all(test, unix))]
#[path = "auth_terminal_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "auth_terminal_backpressure_tests.rs"]
mod backpressure_tests;
