use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use tokio::sync::mpsc as tokio_mpsc;

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentListSessionsResult, ConfigOptionsCatalog};

const LIST_SESSIONS_TIMEOUT: Duration = Duration::from_secs(15);

#[cfg(not(test))]
fn config_options_timeout() -> Duration {
    Duration::from_secs(5)
}

#[cfg(test)]
fn config_options_timeout() -> Duration {
    Duration::from_millis(50)
}

#[derive(Clone)]
pub(super) struct AcpOptionsSessionClient {
    command_tx: tokio_mpsc::UnboundedSender<AcpOptionsCommand>,
}

impl AcpOptionsSessionClient {
    pub(super) fn config_options(&self) -> Result<ConfigOptionsCatalog, RuntimeError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.command_tx
            .send(AcpOptionsCommand::Get { reply_tx })
            .map_err(|_| {
                RuntimeError::NotReady("ACP options session worker stopped".to_string())
            })?;
        recv_options_reply(reply_rx, config_options_timeout(), "request")
    }

    pub(super) fn set_config_option(
        &self,
        config_id: String,
        value: String,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.command_tx
            .send(AcpOptionsCommand::Set {
                config_id,
                value,
                reply_tx,
            })
            .map_err(|_| {
                RuntimeError::NotReady("ACP options session worker stopped".to_string())
            })?;
        recv_options_reply(reply_rx, config_options_timeout(), "update")
    }

    pub(super) fn list_sessions(
        &self,
        agent_id: String,
        cwd: PathBuf,
        cursor: Option<String>,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.command_tx
            .send(AcpOptionsCommand::List {
                agent_id,
                cwd,
                cursor,
                reply_tx,
            })
            .map_err(|_| {
                RuntimeError::NotReady("ACP options session worker stopped".to_string())
            })?;
        reply_rx
            .recv_timeout(LIST_SESSIONS_TIMEOUT)
            .map_err(|error| {
                RuntimeError::NotReady(format!("ACP session list timed out: {error}"))
            })?
    }

    pub(super) fn close(&self) -> Result<(), RuntimeError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.command_tx
            .send(AcpOptionsCommand::Close { reply_tx })
            .map_err(|_| {
                RuntimeError::NotReady("ACP options session worker stopped".to_string())
            })?;
        reply_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| {
                RuntimeError::NotReady(format!("ACP options close timed out: {error}"))
            })?
    }
}

fn recv_options_reply<T>(
    reply_rx: mpsc::Receiver<Result<T, RuntimeError>>,
    timeout: Duration,
    operation: &'static str,
) -> Result<T, RuntimeError> {
    match reply_rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(RuntimeError::NotReady(format!(
            "ACP options {operation} timed out"
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeError::NotReady(
            "ACP options session worker stopped".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests;

pub(super) struct AcpOptionsCommandReceiver(
    pub(super) tokio_mpsc::UnboundedReceiver<AcpOptionsCommand>,
);

pub(super) fn options_session_channel() -> (AcpOptionsSessionClient, AcpOptionsCommandReceiver) {
    let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
    (
        AcpOptionsSessionClient { command_tx },
        AcpOptionsCommandReceiver(command_rx),
    )
}

pub(super) enum AcpOptionsCommand {
    Get {
        reply_tx: mpsc::Sender<Result<ConfigOptionsCatalog, RuntimeError>>,
    },
    Set {
        config_id: String,
        value: String,
        reply_tx: mpsc::Sender<Result<ConfigOptionsCatalog, RuntimeError>>,
    },
    List {
        agent_id: String,
        cwd: PathBuf,
        cursor: Option<String>,
        reply_tx: mpsc::Sender<Result<AgentListSessionsResult, RuntimeError>>,
    },
    Close {
        reply_tx: mpsc::Sender<Result<(), RuntimeError>>,
    },
}
