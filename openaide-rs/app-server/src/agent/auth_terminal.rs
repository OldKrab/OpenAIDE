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
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use super::{acp_agent_config::AcpAgentConfig, TurnCancellation};
use crate::client_lifecycle::{AppServerTime, Delivery};
use crate::logging;
use crate::protocol::errors::RuntimeError;
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
    ) -> Result<(), RuntimeError> {
        let pair = native_pty_system()
            .openpty(size(80, 24))
            .map_err(|_| terminal_error())?;
        let mut command = CommandBuilder::new(&config.command);
        command.args(&config.args);
        command.env("TERM", "xterm-256color");
        for (key, value) in &config.env {
            command.env(key, value);
        }
        let mut child = LoginChild {
            child: pair
                .slave
                .spawn_command(command)
                .map_err(|_| terminal_error())?,
            running: true,
        };
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|_| terminal_error())?;
        let mut writer = pair.master.take_writer().map_err(|_| terminal_error())?;
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
                return Err(terminal_error());
            }
            let exited = child.child.try_wait().map_err(|_| terminal_error())?;
            if exited.is_some() {
                child.kill_group();
                // Never signal this PID after awaiting another browser exchange: it has
                // been reaped and the OS may reuse it while the browser is disconnected.
                child.running = false;
            }
            let mut output = Vec::new();
            while output.len() < 32768 {
                let Ok(bytes) = receive.try_recv() else { break };
                output.extend(bytes);
            }
            let response = self.exchange(
                ShellAuthTerminalParams {
                    agent_id: config.agent_id.clone(),
                    terminal_id: terminal_id.to_string(),
                    output: STANDARD.encode(output),
                    exited: exited.is_some(),
                },
                cancellation,
            )?;
            if response.cancel {
                return Err(terminal_error());
            }
            if let Some(status) = exited {
                return if status.success() {
                    Ok(())
                } else {
                    Err(terminal_error())
                };
            }
            if response.input.len() > 32768
                || !(2..=500).contains(&response.cols)
                || !(2..=200).contains(&response.rows)
            {
                return Err(RuntimeError::InvalidParams(
                    "authentication terminal input".into(),
                ));
            }
            if dimensions != (response.cols, response.rows) {
                pair.master
                    .resize(size(response.cols, response.rows))
                    .map_err(|_| terminal_error())?;
                dimensions = (response.cols, response.rows);
            }
            let input = STANDARD
                .decode(response.input)
                .map_err(|_| terminal_error())?;
            writer
                .write_all(&input)
                .and_then(|_| writer.flush())
                .map_err(|_| terminal_error())?;
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
        logging::info(
            "agent_auth_terminal_completed",
            serde_json::json!({
                "operation_id": terminal_id, "duration_ms": started.elapsed().as_millis(),
                "outcome": if cancellation.is_cancelled() { "cancelled" } else if result.is_ok() { "success" } else { "failure" },
                "error_kind": result.as_ref().err().map(RuntimeError::reason), "attempt": 1,
            }),
        );
        result
    }
}

fn terminal_error() -> RuntimeError {
    RuntimeError::NotReady("Agent authentication terminal ended without successful sign-in".into())
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols,
        rows,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Teardown is unconditional: cancellation, disconnect, malformed responses and spawn/setup
/// failures must never leave a credential-taking process waiting unattended.
struct LoginChild {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    running: bool,
}
impl LoginChild {
    fn kill_group(&self) {
        #[cfg(unix)]
        if let Some(pid) = self.child.process_id() {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}
impl Drop for LoginChild {
    fn drop(&mut self) {
        // PTY launch creates a fresh session/process group. Kill that owned group too:
        // package launchers can exit while their credential-taking child stays alive.
        if self.running {
            self.kill_group();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(all(test, unix))]
#[path = "auth_terminal_tests.rs"]
mod tests;
